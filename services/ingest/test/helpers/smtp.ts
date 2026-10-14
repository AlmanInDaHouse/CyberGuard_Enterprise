import { type AddressInfo, type Socket, createServer } from "node:net";
import { TLSSocket } from "node:tls";
import type { LoopbackCert } from "./tls-cert.js";

export interface FakeSmtpOptions {
  /** Advertise AUTH in the EHLO reply: `PLAIN LOGIN` when `true`, else these mechanisms. */
  offerAuth?: boolean | string;
  /**
   * Offer STARTTLS with this certificate. Before TLS the EHLO reply names STARTTLS and
   * no AUTH; after TLS it names the AUTH of `offerAuth`, as a server that takes
   * credentials only over TLS does.
   */
  starttls?: LoopbackCert;
  /** Name STARTTLS and AUTH in the EHLO reply, then refuse STARTTLS with 454. */
  refuseStarttls?: boolean;
  /** Refuse EHLO with 502, so the client falls back to HELO. */
  refuseEhlo?: boolean;
  /** Close each connection at once, before any greeting. */
  closeBeforeGreeting?: boolean;
  /** The reply to RCPT TO (default `250 OK`). */
  rcptReply?: string;
  /** Delay each reply after the greeting by this many ms, a slow but live server. */
  replyDelayMs?: number;
}

function authLine(offerAuth: boolean | string | undefined): string {
  if (offerAuth === undefined || offerAuth === false) return "";
  return `250-AUTH ${offerAuth === true ? "PLAIN LOGIN" : offerAuth}\r\n`;
}

export interface FakeSmtp {
  port: number;
  /** The command lines received, in order (message data excluded). */
  commands: string[];
  /** The commands received over TLS, in order: those after a completed STARTTLS. */
  commandsOverTls: string[];
  close(): Promise<void>;
}

/**
 * A minimal SMTP server on 127.0.0.1 for the soar_ac suites (SPEC-022): it records
 * the commands it receives, and offers STARTTLS only when `opts.starttls` is set.
 * Its behaviour is set by `opts`.
 */
export async function fakeSmtp(opts: FakeSmtpOptions = {}): Promise<FakeSmtp> {
  const commands: string[] = [];
  const commandsOverTls: string[] = [];
  const sockets = new Set<Socket>();
  const server = createServer((plain) => {
    sockets.add(plain);
    if (opts.closeBeforeGreeting) {
      plain.end();
      return;
    }
    let io: Socket = plain;
    let secure = false;
    let buffered = "";
    let inData = false;
    // AUTH LOGIN's two 334 prompts: 1 awaits the user name, 2 the password.
    let loginStep = 0;
    plain.write("220 fake ESMTP\r\n");
    const reply = (text: string, end = false) => {
      const target = io;
      const send = () => {
        if (target.destroyed) return;
        if (end) target.end(text);
        else target.write(text);
      };
      if (opts.replyDelayMs) setTimeout(send, opts.replyDelayMs);
      else send();
    };
    const ehlo = () => {
      if (opts.refuseStarttls) {
        return `250-fake\r\n250-STARTTLS\r\n${authLine(opts.offerAuth)}250 OK\r\n`;
      }
      if (opts.starttls && !secure) return "250-fake\r\n250-STARTTLS\r\n250 OK\r\n";
      return `250-fake\r\n${authLine(opts.offerAuth)}250 OK\r\n`;
    };
    const onData = (chunk: Buffer) => {
      buffered += chunk.toString("utf8");
      let end = buffered.indexOf("\r\n");
      while (end >= 0) {
        const line = buffered.slice(0, end);
        buffered = buffered.slice(end + 2);
        end = buffered.indexOf("\r\n");
        if (inData) {
          if (line === ".") {
            inData = false;
            reply("250 queued\r\n");
          }
          continue;
        }
        commands.push(line);
        if (secure) commandsOverTls.push(line);
        if (loginStep > 0) {
          loginStep = loginStep === 1 ? 2 : 0;
          reply(loginStep === 2 ? "334 UGFzc3dvcmQ6\r\n" : "235 accepted\r\n");
        } else if (/^STARTTLS$/i.test(line) && opts.refuseStarttls) {
          reply("454 4.7.0 TLS not available\r\n");
        } else if (/^STARTTLS$/i.test(line) && opts.starttls && !secure) {
          // The reply goes in clear text; the next bytes are the TLS handshake.
          plain.removeListener("data", onData);
          plain.write("220 ready to start TLS\r\n", () => {
            const tlsSocket = new TLSSocket(plain, {
              isServer: true,
              key: opts.starttls?.key,
              cert: opts.starttls?.cert,
            });
            tlsSocket.on("error", () => plain.destroy());
            // Whatever arrives from here on came through the TLS session.
            secure = true;
            tlsSocket.on("data", onData);
            io = tlsSocket;
            buffered = "";
          });
          return;
        } else if (/^AUTH LOGIN$/i.test(line)) {
          loginStep = 1;
          reply("334 VXNlcm5hbWU6\r\n");
        } else if (/^EHLO/i.test(line)) {
          if (opts.refuseEhlo) reply("502 5.5.2 EHLO not supported\r\n");
          else reply(ehlo());
        } else if (/^HELO/i.test(line)) reply("250 fake\r\n");
        else if (/^AUTH/i.test(line)) reply("235 accepted\r\n");
        else if (/^RCPT/i.test(line)) reply(`${opts.rcptReply ?? "250 OK"}\r\n`);
        else if (/^DATA/i.test(line)) {
          inData = true;
          reply("354 go on\r\n");
        } else if (/^QUIT/i.test(line)) reply("221 bye\r\n", true);
        else reply("250 OK\r\n");
      }
    };
    plain.on("data", onData);
  });
  await new Promise<void>((resolve) => server.listen({ host: "127.0.0.1", port: 0 }, resolve));
  return {
    port: (server.address() as AddressInfo).port,
    commands,
    commandsOverTls,
    close: () =>
      new Promise<void>((resolve) => {
        for (const socket of sockets) socket.destroy();
        server.close(() => resolve());
      }),
  };
}
