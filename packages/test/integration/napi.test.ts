/**
 * napi smoke test: verify native addons load and work
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { HttpClient, JsHttpResponse } from '@eric8810/napi-http'
import { WsClient } from '@eric8810/napi-ws'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createWSTestServer, type WSTestServer } from '../servers/ws-server.js'

let httpServer: TestServer
let wsServer: WSTestServer

beforeAll(async () => {
  httpServer = await createHttpTestServer()
  wsServer = await createWSTestServer()
})

afterAll(async () => {
  await httpServer?.close()
  await wsServer?.close()
})

describe('@eric8810/napi-http', () => {
  it('creates HttpClient from JSON config', () => {
    const client = new HttpClient(JSON.stringify({
      base_url: '',
      connect_timeout_ms: 5000,
      response_timeout_ms: 30000,
    }))
    expect(client).toBeDefined()
  })

  it('makes a GET request and receives response', async () => {
    const client = new HttpClient(JSON.stringify({
      base_url: `http://localhost:${httpServer.port}`,
      connect_timeout_ms: 5000,
      response_timeout_ms: 30000,
    }))
    const resp: JsHttpResponse = await client.get('/channels')
    expect(resp.status).toBe(200)
  })
})

describe('@eric8810/napi-ws', () => {
  it('creates WsClient from JSON config', () => {
    const ws = new WsClient(JSON.stringify({
      urls: [`ws://localhost:${wsServer.port}`],
      per_message_deflate: false,
      handshake_timeout_ms: 5000,
      reconnect: null,
      race_count: 1,
    }))
    expect(ws).toBeDefined()
  })

  it('receives events via callback', async () => {
    const events: string[] = []
    const ws = new WsClient(JSON.stringify({
      urls: [`ws://localhost:${wsServer.port}`],
      per_message_deflate: false,
      handshake_timeout_ms: 10000,
      reconnect: null,
      race_count: 1,
    }), (e: string) => events.push(e))
    // Wait for async connect
    await new Promise(r => setTimeout(r, 500))
    ws.send('hello')
    await new Promise(r => setTimeout(r, 200))
    expect(ws).toBeDefined()
  })

  it('closes cleanly', () => {
    const ws = new WsClient(JSON.stringify({
      urls: [`ws://localhost:${wsServer.port}`],
      per_message_deflate: false,
      handshake_timeout_ms: 5000,
      reconnect: null,
      race_count: 1,
    }))
    expect(() => ws.close()).not.toThrow()
  })
})
