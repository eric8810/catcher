import http from 'node:http'

import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import {
  CatcherError,
  HttpClient,
  type CatcherErrorSnapshot,
} from '@eric8810/catcher-napi-http'

let timeoutServer: http.Server
let timeoutServerPort: number

beforeAll(async () => {
  timeoutServer = http.createServer(() => {
    // Intentionally leave the response open until Catcher's request timeout fires.
  })
  await new Promise<void>((resolve) => {
    timeoutServer.listen(0, '127.0.0.1', resolve)
  })
  timeoutServerPort = (timeoutServer.address() as import('node:net').AddressInfo).port
})

afterAll(async () => {
  timeoutServer.closeAllConnections()
  await new Promise<void>((resolve, reject) => {
    timeoutServer.close((error) => error ? reject(error) : resolve())
  })
})

describe('@eric8810/catcher-napi-http error contract', () => {
  it('exposes invalid native configuration as a structured error', () => {
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

  it('classifies a refused connection as retryable connection failure', async () => {
    const client = new HttpClient({
      base_url: 'http://127.0.0.1:0',
      connect_timeout_ms: 500,
      response_timeout_ms: 500,
    })

    const error = await client.get('/unreachable').catch((reason) => reason)

    expect(error).toBeInstanceOf(CatcherError)
    expect(error).toMatchObject({
      code: 'CONNECTION_ERROR',
      phase: 'connect',
      retryable: true,
    })
  })

  it('preserves retry attempts and the structured final cause', async () => {
    const client = new HttpClient({
      base_url: 'http://127.0.0.1:0',
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
    const lastError = error.details.lastError as CatcherErrorSnapshot

    expect(error).toBeInstanceOf(CatcherError)
    expect(error).toMatchObject({
      code: 'RETRY_EXHAUSTED',
      phase: 'request',
      retryable: false,
      details: { attempts: 2 },
    })
    expect(lastError).toMatchObject({
      code: 'CONNECTION_ERROR',
      phase: 'connect',
      retryable: true,
    })
    expect(lastError.details.reason).toContain('connect')
    expect(error.message).not.toContain('Request failed after')
    expect(JSON.stringify(error)).not.toContain('must-not-leak')
  })

  it('classifies a response timeout without parsing its message', async () => {
    const client = new HttpClient({
      base_url: `http://127.0.0.1:${timeoutServerPort}`,
      connect_timeout_ms: 500,
      response_timeout_ms: 50,
    })

    const error = await client.get('/never-responds').catch((reason) => reason)

    expect(error).toBeInstanceOf(CatcherError)
    expect(error).toMatchObject({
      code: 'REQUEST_TIMEOUT',
      phase: 'request',
      retryable: true,
      details: { timeoutMs: 50 },
    })
  })
})
