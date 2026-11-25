// The sign-in link redirects to `/#session=<token>`. The token is kept for
// reloads and sent on every call alongside the HttpOnly cookie the sign-in
// set.

/**
 * Where the token is kept. Browsers keep `localStorage` per origin, port
 * included, so each server's token stays its own.
 */
export const STORAGE_KEY = "totalfloat-session";

/**
 * The session token: taken from a `#session=` fragment, which is then
 * removed from the address bar, or the one kept from earlier.
 */
export function takeSession(
  location: Pick<Location, "hash">,
  storage: Pick<Storage, "getItem" | "setItem">,
  forget: () => void,
): string | null {
  const fragment = new URLSearchParams(location.hash.replace(/^#/, ""));
  const given = fragment.get("session");
  if (given) {
    storage.setItem(STORAGE_KEY, given);
    forget();
    return given;
  }
  return storage.getItem(STORAGE_KEY);
}
