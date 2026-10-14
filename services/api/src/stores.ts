import { StoreUnavailableError } from "./errors.js";

/**
 * Run one store call; its failure is the store's, whatever the client threw
 * (SPEC-009 Amendment 2026-10-14 §3; SPEC-008 Amendment 2026-10-15 items 14 and
 * 15). An error the caller raises itself (an AuthError, say) is raised outside the
 * call, so it is never wrapped.
 */
export async function fromStore<T>(call: () => Promise<T>): Promise<T> {
  try {
    return await call();
  } catch (err) {
    throw new StoreUnavailableError(err);
  }
}

/**
 * A view of a store client whose every call fails as the store's: a rejection, a
 * throw, and the calls of a transaction or pipeline it returns. The auth service
 * reads Postgres and Redis through it, so a store that cannot answer on SPEC-008's
 * routes is a 503 (SPEC-008 Amendment 2026-10-15 items 14 and 15).
 */
export function asStore<T extends object>(client: T): T {
  return new Proxy(client, {
    get(target, prop, receiver) {
      const value = Reflect.get(target, prop, target);
      if (typeof value !== "function") return value;
      return (...args: unknown[]) => {
        let result: unknown;
        try {
          result = (value as (...a: unknown[]) => unknown).apply(target, args);
        } catch (err) {
          throw new StoreUnavailableError(err);
        }
        // A chained call (a pipeline's incr().expire()) stays inside the view.
        if (result === target) return receiver;
        if (result instanceof Promise) {
          return result.then(undefined, (err: unknown) => {
            throw new StoreUnavailableError(err);
          });
        }
        if (prop === "multi" || prop === "pipeline") return asStore(result as object);
        return result;
      };
    },
  });
}
