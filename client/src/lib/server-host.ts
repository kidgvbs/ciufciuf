// Host del server a cui il client è autorizzato a parlare.
//
// In produzione il client parla solo con il server che lo ha servito
// (`window.location.origin`); in sviluppo (`vite dev`) `?server=` può puntare
// a un altro server.

export interface HostEnv {
  origin: string;
  dev: boolean;
}

/** Ambiente corrente del browser (origin della pagina, modalità dev di Vite). */
export function currentHostEnv(): HostEnv {
  return {
    origin: globalThis.location?.origin ?? '',
    dev: import.meta.env.DEV,
  };
}

function normalize(host: string): string {
  return host.trim().replace(/\/+$/, '');
}

/** `true` se `host` è l'origin della pagina oppure siamo in modalità dev. */
export function isAllowedHost(host: string, env: HostEnv = currentHostEnv()): boolean {
  if (!host) return false;
  return env.dev || normalize(host) === normalize(env.origin);
}

/** Host da usare: `?server=` se ammesso, altrimenti l'origin della pagina. */
export function resolveServerHost(search: string, env: HostEnv = currentHostEnv()): string {
  const param = new URLSearchParams(search).get('server');
  if (param && isAllowedHost(param, env)) return normalize(param);
  return env.origin;
}
