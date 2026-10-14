import { execFileSync } from 'node:child_process';
import { mkdirSync, renameSync } from 'node:fs';

execFileSync('cargo', ['build', '--manifest-path', 'rust/Cargo.toml', '--locked', '--release', '--target', 'wasm32-unknown-unknown'], { stdio: 'inherit' });
mkdirSync('dist/wasm', { recursive: true });
execFileSync('wasm-bindgen', ['rust/target/wasm32-unknown-unknown/release/caido_dev.wasm', '--target', 'nodejs', '--out-dir', 'dist/wasm', '--out-name', 'caido_dev'], { stdio: 'inherit' });
renameSync('dist/wasm/caido_dev.js', 'dist/wasm/caido_dev.cjs');
