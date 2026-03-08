import sharp from 'sharp';
import { writeFileSync, mkdirSync, existsSync } from 'fs';
import { join } from 'path';
import https from 'https';
import http from 'http';

const WALLPAPERS = [
  { id: 'ipados-orange', url: 'https://4kwallpapers.com/images/wallpapers/ipados-stock-orange-white-background-ipad-ios-13-hd-3208x3208-1551.jpg' },
  { id: 'ipados-blue',   url: 'https://4kwallpapers.com/images/wallpapers/ipados-stock-blue-white-background-ipad-ios-13-hd-3208x3208-1550.jpg' },
  { id: 'blue-waves',    url: 'https://4kwallpapers.com/images/wallpapers/blue-waves-5k-5120x2880-24614.jpg' },
  { id: 'windows-xp',   url: 'https://4kwallpapers.com/images/wallpapers/windows-xp-3840x3840-17062.jpg' },
  { id: 'antelope-canyon', url: 'https://4kwallpapers.com/images/wallpapers/antelope-canyon-3840x2160-13871.jpg' },
  { id: 'windows-7',    url: 'https://4kwallpapers.com/images/wallpapers/windows-7-official-3840x2160-13944.jpg' },
  { id: 'windows-11-colorful', url: 'https://4kwallpapers.com/images/wallpapers/windows-11-stock-official-colorful-3840x2160-5666.jpg' },
  { id: 'big-sur-iridescence', url: 'https://4kwallpapers.com/images/wallpapers/iridescence-macos-big-sur-macbook-pro-multicolor-dark-6016x6016-4036.jpg' },
  { id: 'landscape-rocks', url: 'https://4kwallpapers.com/images/wallpapers/landscape-rocks-6016x6016-11016.jpg' },
  { id: 'lake-mountains', url: 'https://4kwallpapers.com/images/wallpapers/lake-mountains-rocks-sunrise-daylight-scenery-illustration-6016x6016-3773.jpg' },
  { id: 'big-sur-rocks', url: 'https://4kwallpapers.com/images/wallpapers/macos-big-sur-stock-daytime-sedimentary-rocks-daylight-6016x6016-3785.jpg' },
  { id: 'big-sur-waves', url: 'https://4kwallpapers.com/images/wallpapers/waves-macos-big-sur-colorful-dark-5k-6016x6016-4990.jpg' },
  { id: 'sierra-glacier', url: 'https://4kwallpapers.com/images/wallpapers/macos-sierra-glacier-mountains-snow-covered-alpenglow-5120x2880-6420.jpg' },
  { id: 'monterey-dark', url: 'https://4kwallpapers.com/images/wallpapers/macos-monterey-stock-black-dark-mode-layers-5k-6016x6016-5889.jpg' },
];

const TMP_DIR = '/tmp/bg-raw-new';
const PUBLIC_DIR = 'C:/WORK/screen-goated-toolbox/screen-record/public';
const DIST_DIR   = 'C:/WORK/screen-goated-toolbox/src/overlay/screen_record/dist';

mkdirSync(TMP_DIR, { recursive: true });

function download(url, dest) {
  return new Promise((resolve, reject) => {
    if (existsSync(dest)) { console.log(`  cached: ${dest}`); return resolve(dest); }
    const file = import('fs').then(fs => fs.createWriteStream(dest));
    file.then(stream => {
      const proto = url.startsWith('https') ? https : http;
      const req = proto.get(url, {
        headers: { 'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36' }
      }, res => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          stream.close();
          return download(res.headers.location, dest).then(resolve).catch(reject);
        }
        if (res.statusCode !== 200) { stream.close(); return reject(new Error(`HTTP ${res.statusCode} for ${url}`)); }
        res.pipe(stream);
        stream.on('finish', () => { stream.close(); resolve(dest); });
      });
      req.on('error', e => { stream.close(); reject(e); });
    });
  });
}

async function processOne(w) {
  const rawPath = `${TMP_DIR}/${w.id}.raw`;
  console.log(`Downloading ${w.id}...`);
  await download(w.url, rawPath);

  console.log(`  resizing ${w.id}...`);
  const outName = `bg-${w.id}.jpg`;
  const outPublic = join(PUBLIC_DIR, outName);
  const outDist   = join(DIST_DIR,   outName);

  await sharp(rawPath)
    .resize(320, 180, { fit: 'cover', position: 'centre' })
    .jpeg({ quality: 50 })
    .toFile(outPublic);

  // mirror to dist
  const { copyFileSync } = await import('fs');
  copyFileSync(outPublic, outDist);

  const { statSync } = await import('fs');
  const kb = (statSync(outPublic).size / 1024).toFixed(1);
  console.log(`  ✓ ${outName} — ${kb} KB`);
}

for (const w of WALLPAPERS) {
  try {
    await processOne(w);
  } catch (e) {
    console.error(`  ✗ ${w.id}: ${e.message}`);
  }
}

console.log('\nDone!');
