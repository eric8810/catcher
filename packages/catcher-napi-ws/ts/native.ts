import os from 'os'
import path from 'path'

function tryRequire(...paths: string[]): any {
  for (const p of paths) {
    try { return require(p) } catch {}
  }
  return null
}

function isMusl(): boolean {
  const report = (typeof process.report?.getReport === 'function'
    ? process.report.getReport() : null) as any
  if (report?.header?.glibcVersionRuntime) return false
  try {
    const lddPath = require('child_process')
      .execSync('which ldd', { encoding: 'utf8' }).trim()
    return require('fs').readFileSync(lddPath, 'utf8').includes('musl')
  } catch {
    return true
  }
}

function getAbi(): string {
  if (process.env.npm_config_libc) return process.env.npm_config_libc
  const platform = os.platform()
  if (platform === 'win32') return 'msvc'
  if (platform === 'linux') return isMusl() ? 'musl' : 'gnu'
  return ''
}

/**
 * tsup 输出到 dist/，__dirname = dist/，
 * 所以所有路径都需要 path.join(__dirname, '..') 回到包根。
 */
export function loadNativeAddon(pkgName: string): any {
  const platform = os.platform()
  const arch = os.arch()
  const abi = getAbi()
  const platformKey = abi ? `${platform}-${arch}-${abi}` : `${platform}-${arch}`
  const root = path.join(__dirname, '..')

  // 1. optionalDependencies 子包（npm install 自动安装对应平台）
  const subPkg = tryRequire(`@eric8810/${pkgName}-${platformKey}`)
  if (subPkg) return subPkg

  // 2. napi build 生成的 index.js（包根）
  const napiJs = tryRequire(path.join(root, 'index.js'))
  if (napiJs) return napiJs

  // 3. 本地开发：根目录 .node 文件 / cargo build 产物
  const libName = pkgName.replace(/-/g, '_')
  const addon =
    tryRequire(path.join(root, `${pkgName}.${platformKey}.node`)) ??
    tryRequire(path.join(root, `${pkgName}.${platform}-${arch}.node`)) ??
    tryRequire(path.join(root, `${pkgName}.node`)) ??
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
