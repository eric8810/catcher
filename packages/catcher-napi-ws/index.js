const os = require('os')
const path = require('path')

function tryRequire(...paths) {
  for (const p of paths) {
    try { return require(p) } catch {}
  }
  return null
}

const platform = os.platform()
const arch = os.arch()
const pkg = 'catcher-napi-ws'
const dirname = __dirname

let addon =
  // 1. npm prebuilt artifacts dir
  tryRequire(path.join(dirname, 'npm', `${platform}-${arch}`, `${pkg}.node`)) ??
  // 2. local napi build output
  tryRequire(path.join(dirname, `${pkg}.node`)) ??
  // 3. cargo build target (linux .so, handled as .node copy)
  tryRequire(path.join(dirname, 'target', 'release', `lib${pkg.replace(/-/g, '_')}.so`))

if (!addon) {
  throw new Error(`@catcher/napi-ws: native addon not found. Run \`npm run build\` in packages/${pkg}.`)
}

const RawClient = addon.JsWsClient

class WsClient {
  constructor(config, onEvent) {
    const configJson = typeof config === 'string' ? config : JSON.stringify(config)
    this._raw = new RawClient(configJson, onEvent)
  }

  send(data) {
    return this._raw.send(data)
  }

  close() {
    return this._raw.close()
  }
}

module.exports = { WsClient }
