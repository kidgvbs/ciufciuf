// QR code generato nel browser: l'URL (che può contenere un token) non lascia mai il client.

import qrcode from 'qrcode-generator';

/** Data URL (GIF) del QR che codifica `data`. */
export function qrDataUrl(data: string): string {
  const qr = qrcode(0, 'M');
  qr.addData(data);
  qr.make();
  return qr.createDataURL(4, 2);
}
