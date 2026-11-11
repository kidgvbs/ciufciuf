// Host del server a cui il client è autorizzato a parlare.
//
// Il client parla solo con il server che lo ha servito (`window.location.origin`); in sviluppo
// il proxy di Vite inoltra `/api` e `/ws` al server locale, quindi non serve un host diverso.

export interface HostEnv {
  origin: string;
}

/** Ambiente corrente del browser (origin della pagina). */
export function currentHostEnv(): HostEnv {
  return { origin: globalThis.location?.origin ?? '' };
}

function normalize(host: string): string {
  return host.trim().replace(/\/+$/, '');
}

/** `true` se `host` è l'origin della pagina. */
export function isAllowedHost(host: string, env: HostEnv = currentHostEnv()): boolean {
  if (!host) return false;
  return normalize(host) === normalize(env.origin);
}
