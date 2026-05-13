const os = require('os')
const path = require('path')

let addon
try {
  addon = require(`./npm/${os.platform()}-${os.arch()}/catcher-napi-http.node`)
} catch {
  try {
    addon = require(path.join(__dirname, 'catcher-napi-http.node'))
  } catch {
    throw new Error('@catcher/napi-http: native addon not found. Run `npm run build`.')
  }
}

const RawClient = addon.JsHttpClient

class HttpClient {
  constructor(config) {
    const configJson = typeof config === 'string' ? config : JSON.stringify(config)
    this._raw = new RawClient(configJson)
  }

  async get(url, options) {
    return this._raw.get(url, options ?? undefined)
  }

  async post(url, body, options) {
    return this._raw.post(url, body ?? undefined, options ?? undefined)
  }

  async put(url, body, options) {
    if (this._raw.put) return this._raw.put(url, body ?? undefined, options ?? undefined)
    throw new Error('put() requires rebuilt native addon (cargo build)')
  }

  async delete(url, options) {
    if (this._raw.delete) return this._raw.delete(url, options ?? undefined)
    throw new Error('delete() requires rebuilt native addon (cargo build)')
  }

  async patch(url, body, options) {
    if (this._raw.patch) return this._raw.patch(url, body ?? undefined, options ?? undefined)
    throw new Error('patch() requires rebuilt native addon (cargo build)')
  }

  circuitBreakerState() {
    if (this._raw.circuitBreakerState) return this._raw.circuitBreakerState()
    return 'closed' // fallback: old .node without CB state
  }
}

module.exports = { HttpClient }
