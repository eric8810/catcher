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

module.exports = {
  HttpClient: addon.JsHttpClient,
  JsHttpResponse: addon.JsHttpResponse,
}
