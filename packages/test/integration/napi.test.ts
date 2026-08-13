/**
 * napi smoke test: verify native addons load and work
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import {
  CatcherError,
  HttpClient,
  HttpError,
  JsHttpResponse,
} from '@eric8810/catcher-napi-http'
import { WsClient } from '@eric8810/catcher-napi-ws'
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

describe('@eric8810/catcher-napi-http', () => {
  it('creates HttpClient from JSON config', () => {
    const client = new HttpClient(JSON.stringify({
      base_url: '',
      connect_timeout_ms: 5000,
      response_timeout_ms: 30000,
    }))
    expect(client).toBeDefined()
  })

  it('exposes invalid native configuration as a structured CatcherError', () => {
    let error: unknown
    try {
      new HttpClient('{')
    } catch (reason) {
      error = reason
    }

    expect(error).toBeInstanceOf(CatcherError)
    expect(error).toMatchObject({
      name: 'CatcherError',
      code: 'INVALID_CONFIG',
      phase: 'config',
      retryable: false,
    })
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

  it('exposes HTTP status and body as a structured HttpError', async () => {
    const client = new HttpClient({
      base_url: `http://localhost:${httpServer.port}`,
      connect_timeout_ms: 5000,
      response_timeout_ms: 30000,
    })

    const error = await client.get('/not-found').catch((reason) => reason)

    expect(error).toBeInstanceOf(HttpError)
    expect(error).toMatchObject({
      name: 'HttpError',
      status: 404,
    })
    expect(error.body).toContain('not found')
  })

  it('preserves retry attempts and the final transport cause', async () => {
    const listener = await import('node:net').then(({ createServer }) => {
      const server = createServer()
      return new Promise<{ server: import('node:net').Server; port: number }>((resolve) => {
        server.listen(0, '127.0.0.1', () => {
          resolve({
            server,
            port: (server.address() as import('node:net').AddressInfo).port,
          })
        })
      })
    })
    await new Promise<void>((resolve, reject) => {
      listener.server.close((error) => error ? reject(error) : resolve())
    })

    const client = new HttpClient({
      base_url: `http://127.0.0.1:${listener.port}`,
      connect_timeout_ms: 500,
      response_timeout_ms: 500,
      retry: {
        max_attempts: 1,
        min_backoff_ms: 1,
        max_backoff_ms: 1,
        backoff: 'Fixed',
        jitter: false,
      },
    })

    const error = await client
      .get('/unreachable?access_token=must-not-leak')
      .catch((reason) => reason)

    expect(error).toBeInstanceOf(CatcherError)
    expect(error).toMatchObject({
      name: 'CatcherError',
      code: 'RETRY_EXHAUSTED',
      phase: 'request',
      details: {
        attempts: 2,
      },
    })
    expect(error.details.lastError).toContain('connection failed')
    expect(error.message).not.toContain('Request failed after')
    expect(error.message).not.toContain('must-not-leak')
  })

  it('retries HTTP 421 once after resetting only its connection pool', async () => {
    const client = new HttpClient({
      base_url: `http://localhost:${httpServer.port}`,
      connect_timeout_ms: 5000,
      response_timeout_ms: 30000,
    })

    const response = await client.post(
      '/misdirected-once',
      Buffer.from('{"cmid":"client-message-1"}'),
      { contentType: 'application/json' },
    )

    expect(response.status).toBe(201)
    expect(JSON.parse(response.body.toString())).toMatchObject({
      accepted: true,
      attempts: 2,
    })
    expect(client.metrics().httpRetries).toBe(1)
  })
})

describe('@eric8810/catcher-napi-ws', () => {
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
    const events: any[] = []
    let resolveConnected!: () => void
    let rejectConnected!: (error: Error) => void
    const connected = new Promise<void>((resolve, reject) => {
      resolveConnected = resolve
      rejectConnected = reject
    })
    const timeout = setTimeout(() => {
      rejectConnected(new Error(`Timed out waiting for Connected event. Events: ${JSON.stringify(events)}`))
    }, 5_000)

    const ws = new WsClient(JSON.stringify({
      urls: [wsServer.url],
      per_message_deflate: false,
      handshake_timeout_ms: 10000,
      reconnect: null,
      race_count: 1,
    }), (event) => {
      events.push(event)
      if (event.type === 'Connected') {
        clearTimeout(timeout)
        resolveConnected()
      } else if (event.type === 'Error') {
        clearTimeout(timeout)
        rejectConnected(new Error(event.message))
      }
    })

    await connected
    ws.send('hello')
    await new Promise(r => setTimeout(r, 200))
    expect(ws).toBeDefined()
    expect(events.some(e => e.type === 'Connected')).toBe(true)
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
