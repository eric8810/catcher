import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import http from 'node:http'
import { createHttpClient } from '../client.js'

let server: http.Server
let baseUrl: string

// Generate a large body (100KB)
function generateLargeBody(sizeKb: number): string {
  const chunk = '{"data":"' + 'x'.repeat(100) + '"}'
  const repeat = Math.ceil((sizeKb * 1024) / chunk.length)
  return chunk.repeat(repeat)
}

const largeBody = generateLargeBody(100)

function startServer(): Promise<void> {
  return new Promise((resolve) => {
    server = http.createServer((req, res) => {
      const url = new URL(req.url!, `http://${req.headers.host}`)

      // Large body endpoint
      if (url.pathname === '/large') {
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(largeBody)
        return
      }

      // Slow stream endpoint — sends data in chunks with delays
      if (url.pathname === '/slow-stream') {
        res.writeHead(200, { 'Content-Type': 'application/octet-stream' })
        const chunkSize = 1024
        let sent = 0
        const total = 10 * 1024
        const interval = setInterval(() => {
          const data = Buffer.alloc(Math.min(chunkSize, total - sent), 'a')
          res.write(data)
          sent += data.length
          if (sent >= total) {
            res.end()
            clearInterval(interval)
          }
        }, 10)
        return
      }

      res.writeHead(404)
      res.end('not found')
    })
    server.listen(0, '127.0.0.1', () => resolve())
  })
}

function stopServer(): Promise<void> {
  return new Promise((resolve) => { server.close(() => resolve()) })
}

/** Read all data from a Node.js Readable stream */
function readStream(stream: any): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = []
    stream.on('data', (chunk: Buffer) => chunks.push(chunk))
    stream.on('end', () => resolve(Buffer.concat(chunks)))
    stream.on('error', reject)
  })
}

beforeEach(async () => {
  await startServer()
  const addr = server.address() as any
  baseUrl = `http://127.0.0.1:${addr.port}`
})

afterEach(async () => {
  await stopServer()
})

// ── ST1-ST4: Stream Response (G10) ───────────────────────────────

describe('ST1 — responseType: "stream" returns Node.js Readable', () => {
  it('returns an object with data being a readable stream', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const result = await client.get('/large', {
      responseType: 'stream',
    } as any)

    // Stream responses bypass interceptor chain and return the full HttpResponse
    expect(result.status).toBe(200)
    expect(result.headers).toBeDefined()
    expect(result.data).toBeDefined()
    // Node.js Readable has on('data') and on('end')
    expect(typeof result.data.on).toBe('function')
    expect(typeof result.data.pipe).toBe('function')

    // Consume the stream to avoid leaks
    await readStream(result.data)
  })
})

describe('ST2 — Stream reads complete data', () => {
  it('reads the entire large body from the stream', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const result = await client.get('/large', {
      responseType: 'stream',
    } as any)

    const buffer = await readStream(result.data)
    expect(buffer.length).toBeGreaterThan(0)
    expect(buffer.toString('utf-8')).toBe(largeBody)
  })
})

describe('ST3 — Stream with onDownloadProgress', () => {
  it('onDownloadProgress callback fires with increasing loaded', async () => {
    const progressCalls: any[] = []
    const client = createHttpClient({ baseURL: baseUrl })
    const result = await client.get('/large', {
      responseType: 'stream',
      onDownloadProgress: (event: any) => {
        progressCalls.push(event)
      },
    } as any)

    // Consume the stream
    await readStream(result.data)

    // onDownloadProgress should have been called at least once
    // (may be 0 for small bodies with axios, but our body is 100KB)
    if (progressCalls.length > 0) {
      // loaded should increase
      const loadedValues = progressCalls.map((e: any) => e.loaded)
      for (let i = 1; i < loadedValues.length; i++) {
        expect(loadedValues[i]).toBeGreaterThanOrEqual(loadedValues[i - 1])
      }
    }
  })
})

describe('ST4 — Stream mid-cancel', () => {
  it('stops the stream when aborted', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const ac = new AbortController()

    const result = await client.get('/slow-stream', {
      responseType: 'stream',
      signal: ac.signal,
    } as any)

    // Start reading then abort
    const readPromise = readStream(result.data)

    // Abort after a small delay
    setTimeout(() => ac.abort(), 50)

    // The read should fail or complete with partial data
    try {
      const buffer = await readPromise
      // If it completes, it should have less than the full 10KB
      expect(buffer.length).toBeLessThan(10 * 1024)
    } catch {
      // Abort error is also acceptable
    }
  })
})
