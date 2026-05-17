import os from 'os'
import path from 'path'

function tryRequire(...paths: string[]): any {
  for (const p of paths) {
    try { return require(p) } catch {}
  }
  return null
}

/**
 * 加载 napi 原生模块
 *
 * tsup 输出到 dist/，__dirname = dist/，
 * 所以所有路径都需要 path.join(__dirname, '..') 回到包根。
 */
export function loadNativeAddon(pkgName: string): any {
  const platform = os.platform()
  const arch = os.arch()
  // __dirname = dist/，root = 包根目录
  const root = path.join(__dirname, '..')

  // 1. index.js — napi build 生成的入口（在包根）
  const napiJs = tryRequire(path.join(root, 'index.js'))
  if (napiJs) return napiJs

  // 2. 预编译二进制 → 3. 根目录 .node → 4. cargo build 产物
  const libName = pkgName.replace(/-/g, '_')
  const addon =
    tryRequire(path.join(root, 'npm', `${platform}-${arch}`, `${pkgName}.node`)) ??
    tryRequire(path.join(root, `${pkgName}.node`)) ??
    tryRequire(path.join(root, `${pkgName}.${platform}-${arch}.node`)) ??
    tryRequire(path.join(root, 'target', 'release', `lib${libName}.so`)) ??
    tryRequire(path.join(root, 'target', 'release', `lib${libName}.dylib`)) ??
    tryRequire(path.join(root, 'target', 'release', `${libName}.dll`))

  if (!addon) {
    throw new Error(
      `@eric8810/${pkgName}: native addon not found.\n` +
      `Run \`npm run build\` in packages/${pkgName} (requires Rust).`
    )
  }

  return addon
}
