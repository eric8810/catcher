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

module.exports = {
  WsClient: addon.JsWsClient,
}
