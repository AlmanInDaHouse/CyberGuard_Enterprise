import { type AddressInfo, type Socket, createServer } from "node:net";

export interface FakeSmtpOptions {
  /** Advertise AUTH in the EHLO reply: `PLAIN LOGIN` when `true`, else these mechanisms. */
  offerAuth?: boolean | string;
  /** Refuse EHLO with 502, so the client falls back to HELO. */
  refuseEhlo?: boolean;
  /** Close each connection at once, before any greeting. */
  closeBeforeGreeting?: boolean;
  /** The reply to RCPT TO (default `250 OK`). */
  rcptReply?: string;
}

function authLine(offerAuth: boolean | string | undefined): string {
  if (offerAuth === undefined || offerAuth === false) return "";
  return `250-AUTH ${offerAuth === true ? "PLAIN LOGIN" : offerAuth}\r\n`;
}

export interface FakeSmtp {
  port: number;
  /** The command lines received, in order (message data excluded). */
  commands: string[];
  close(): Promise<void>;
}

/**
 * A minimal SMTP server on 127.0.0.1 for the soar_ac suites (SPEC-022): it records
 * the commands it receives and offers no STARTTLS. Its behaviour is set by `opts`.
 */
export async function fakeSmtp(opts: FakeSmtpOptions = {}): Promise<FakeSmtp> {
  const commands: string[] = [];
  const sockets = new Set<Socket>();
  const server = createServer((socket) => {
    sockets.add(socket);
    if (opts.closeBeforeGreeting) {
      socket.end();
      return;
    }
    let buffered = "";
    let inData = false;
    // AUTH LOGIN's two 334 prompts: 1 awaits the user name, 2 the password.
    let loginStep = 0;
    socket.write("220 fake ESMTP\r\n");
    socket.on("data", (chunk) => {
      buffered += chunk.toString("utf8");
      let end = buffered.indexOf("\r\n");
      while (end >= 0) {
        const line = buffered.slice(0, end);
        buffered = buffered.slice(end + 2);
        end = buffered.indexOf("\r\n");
        if (inData) {
          if (line === ".") {
            inData = false;
            socket.write("250 queued\r\n");
          }
          continue;
        }
        commands.push(line);
        if (loginStep > 0) {
          loginStep = loginStep === 1 ? 2 : 0;
          socket.write(loginStep === 2 ? "334 UGFzc3dvcmQ6\r\n" : "235 accepted\r\n");
        } else if (/^AUTH LOGIN$/i.test(line)) {
          loginStep = 1;
          socket.write("334 VXNlcm5hbWU6\r\n");
        } else if (/^EHLO/i.test(line)) {
          if (opts.refuseEhlo) socket.write("502 5.5.2 EHLO not supported\r\n");
          else {
            socket.write(`250-fake\r\n${authLine(opts.offerAuth)}250 OK\r\n`);
          }
        } else if (/^HELO/i.test(line)) socket.write("250 fake\r\n");
        else if (/^AUTH/i.test(line)) socket.write("235 accepted\r\n");
        else if (/^RCPT/i.test(line)) socket.write(`${opts.rcptReply ?? "250 OK"}\r\n`);
        else if (/^DATA/i.test(line)) {
          inData = true;
          socket.write("354 go on\r\n");
        } else if (/^QUIT/i.test(line)) socket.end("221 bye\r\n");
        else socket.write("250 OK\r\n");
      }
    });
  });
  await new Promise<void>((resolve) => server.listen({ host: "127.0.0.1", port: 0 }, resolve));
  return {
    port: (server.address() as AddressInfo).port,
    commands,
    close: () =>
      new Promise<void>((resolve) => {
        for (const socket of sockets) socket.destroy();
        server.close(() => resolve());
      }),
  };
}
