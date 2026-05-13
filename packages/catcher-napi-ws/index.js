const os = require('os')
const path = require('path')

let addon
try {
  addon = require(`./npm/${os.platform()}-${os.arch()}/catcher-napi-ws.node`)
} catch {
  try {
    addon = require(path.join(__dirname, 'catcher-napi-ws.node'))
  } catch {
    throw new Error('@catcher/napi-ws: native addon not found. Run `npm run build`.')
  }
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
