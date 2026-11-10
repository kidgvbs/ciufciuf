// Host del server a cui il client è autorizzato a parlare.
//
// In produzione il client parla solo con il server che lo ha servito
// (`window.location.origin`); in sviluppo (`vite dev`) `?server=` può puntare
// a un altro server, ma solo su localhost o su una rete privata.

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

/** `true` se `hostname` è `localhost` o un IPv4 in 127/8, 10/8, 172.16/12, 192.168/16. */
function isPrivateHostname(hostname: string): boolean {
  if (hostname === 'localhost') return true;
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(hostname);
  if (!m) return false;
  const [a, b, c, d] = m.slice(1).map(Number) as [number, number, number, number];
  if ([a, b, c, d].some((o) => o > 255)) return false;
  return a === 127 || a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168);
}

function isPrivateServer(host: string): boolean {
  try {
    const url = new URL(host);
    return (url.protocol === 'http:' || url.protocol === 'https:') && isPrivateHostname(url.hostname);
  } catch {
    return false;
  }
}

/** `true` se `host` è l'origin della pagina oppure, in modalità dev, un server locale/privato. */
export function isAllowedHost(host: string, env: HostEnv = currentHostEnv()): boolean {
  if (!host) return false;
  return normalize(host) === normalize(env.origin) || (env.dev && isPrivateServer(host));
}

/** Host da usare: `?server=` se ammesso, altrimenti l'origin della pagina. */
export function resolveServerHost(search: string, env: HostEnv = currentHostEnv()): string {
  const param = new URLSearchParams(search).get('server');
  if (param && isAllowedHost(param, env)) return normalize(param);
  return env.origin;
}
