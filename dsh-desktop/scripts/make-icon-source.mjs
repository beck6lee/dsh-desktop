import sharp from 'sharp';
import { mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const src = path.resolve(here, '../../assets/whale-girl/whale-girl-transparent.png');
const outDir = path.resolve(here, '../icons');
await mkdir(outDir, { recursive: true });
await sharp(src)
  .resize(1024, 1024, { fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } })
  .png()
  .toFile(path.join(outDir, 'icon-source.png'));
console.log('icon source written:', path.join(outDir, 'icon-source.png'));
