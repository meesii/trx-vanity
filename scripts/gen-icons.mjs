import { mkdirSync, readFileSync } from 'fs';
import { dirname, resolve } from 'path';
import sharp from 'sharp';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');
const svg = readFileSync(resolve(root, 'icon-draft.svg'));
const out = resolve(root, 'src-tauri/icons');
const density = 1200;

const tasks = [
    { name: '32x32.png', size: 32 },
    { name: '64x64.png', size: 64 },
    { name: '128x128.png', size: 128 },
    { name: '128x128@2x.png', size: 256 },
    { name: 'icon.png', size: 512 },
    { name: 'Square30x30Logo.png', size: 30 },
    { name: 'Square44x44Logo.png', size: 44 },
    { name: 'Square71x71Logo.png', size: 71 },
    { name: 'Square89x89Logo.png', size: 89 },
    { name: 'Square107x107Logo.png', size: 107 },
    { name: 'Square142x142Logo.png', size: 142 },
    { name: 'Square150x150Logo.png', size: 150 },
    { name: 'Square284x284Logo.png', size: 284 },
    { name: 'Square310x310Logo.png', size: 310 },
    { name: 'StoreLogo.png', size: 50 },
];

const android_sizes = [
    { folder: 'mipmap-hdpi', size: 72 },
    { folder: 'mipmap-mdpi', size: 48 },
    { folder: 'mipmap-xhdpi', size: 96 },
    { folder: 'mipmap-xxhdpi', size: 144 },
    { folder: 'mipmap-xxxhdpi', size: 192 },
];

const ios_sizes = [
    { name: 'AppIcon-20x20@1x.png', size: 20 },
    { name: 'AppIcon-20x20@2x.png', size: 40 },
    { name: 'AppIcon-20x20@2x-1.png', size: 40 },
    { name: 'AppIcon-20x20@3x.png', size: 60 },
    { name: 'AppIcon-29x29@1x.png', size: 29 },
    { name: 'AppIcon-29x29@2x.png', size: 58 },
    { name: 'AppIcon-29x29@2x-1.png', size: 58 },
    { name: 'AppIcon-29x29@3x.png', size: 87 },
    { name: 'AppIcon-40x40@1x.png', size: 40 },
    { name: 'AppIcon-40x40@2x.png', size: 80 },
    { name: 'AppIcon-40x40@2x-1.png', size: 80 },
    { name: 'AppIcon-40x40@3x.png', size: 120 },
    { name: 'AppIcon-60x60@2x.png', size: 120 },
    { name: 'AppIcon-60x60@3x.png', size: 180 },
    { name: 'AppIcon-76x76@1x.png', size: 76 },
    { name: 'AppIcon-76x76@2x.png', size: 152 },
    { name: 'AppIcon-83.5x83.5@2x.png', size: 167 },
    { name: 'AppIcon-512@2x.png', size: 1024 },
];

async function run() {
    for (const t of tasks) {
        await sharp(svg, { density }).resize(t.size, t.size).png().toFile(resolve(out, t.name));
        console.log(`  ${t.name}`);
    }

    for (const a of android_sizes) {
        const dir = resolve(out, 'android', a.folder);
        mkdirSync(dir, { recursive: true });
        await sharp(svg, { density }).resize(a.size, a.size).png().toFile(resolve(dir, 'ic_launcher.png'));
        await sharp(svg, { density }).resize(a.size, a.size).png().toFile(resolve(dir, 'ic_launcher_round.png'));
        await sharp(svg, { density }).resize(a.size, a.size).png().toFile(resolve(dir, 'ic_launcher_foreground.png'));
        console.log(`  android/${a.folder}`);
    }

    const iosDir = resolve(out, 'ios');
    mkdirSync(iosDir, { recursive: true });
    for (const i of ios_sizes) {
        await sharp(svg, { density }).resize(i.size, i.size).png().toFile(resolve(iosDir, i.name));
        console.log(`  ios/${i.name}`);
    }

    /* ICO: 16, 24, 32, 48, 64, 256 */
    const ico_sizes = [16, 24, 32, 48, 64, 256];
    const ico_bufs = [];
    for (const s of ico_sizes) {
        ico_bufs.push(await sharp(svg, { density }).resize(s, s).png().toBuffer());
    }
    const ico = build_ico(ico_bufs, ico_sizes);
    const { writeFileSync } = await import('fs');
    writeFileSync(resolve(out, 'icon.ico'), ico);
    console.log('  icon.ico');

    /* ICNS: 简单方案，用最大的 512 PNG 作为 icns 的 ic09 */
    const buf512 = await sharp(svg, { density }).resize(512, 512).png().toBuffer();
    const icns = build_icns(buf512);
    writeFileSync(resolve(out, 'icon.icns'), icns);
    console.log('  icon.icns');

    console.log('done');
}

function build_ico(buffers, sizes) {
    const count = buffers.length;
    const header_size = 6 + count * 16;
    let offset = header_size;
    const entries = [];
    for (let i = 0; i < count; i++) {
        const s = sizes[i] >= 256 ? 0 : sizes[i];
        const buf = buffers[i];
        entries.push({ w: s, h: s, offset, size: buf.length });
        offset += buf.length;
    }
    const out = Buffer.alloc(offset);
    out.writeUInt16LE(0, 0);
    out.writeUInt16LE(1, 2);
    out.writeUInt16LE(count, 4);
    for (let i = 0; i < count; i++) {
        const e = entries[i];
        const pos = 6 + i * 16;
        out.writeUInt8(e.w, pos);
        out.writeUInt8(e.h, pos + 1);
        out.writeUInt8(0, pos + 2);
        out.writeUInt8(0, pos + 3);
        out.writeUInt16LE(1, pos + 4);
        out.writeUInt16LE(32, pos + 6);
        out.writeUInt32LE(e.size, pos + 8);
        out.writeUInt32LE(e.offset, pos + 12);
        buffers[i].copy(out, e.offset);
    }
    return out;
}

function build_icns(png512) {
    const type = Buffer.from('ic09');
    const size = Buffer.alloc(4);
    size.writeUInt32BE(png512.length + 8);
    const entry = Buffer.concat([type, size, png512]);
    const magic = Buffer.from('icns');
    const total = Buffer.alloc(4);
    total.writeUInt32BE(entry.length + 8);
    return Buffer.concat([magic, total, entry]);
}

run().catch(console.error);
