import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import http from 'node:http'
import { createHttpClient } from '../client.js'

let server: http.Server
let baseUrl: string

function startServer(): Promise<void> {
  return new Promise((resolve) => {
    server = http.createServer((req, res) => {
      const url = new URL(req.url!, `http://${req.headers.host}`)

      if (url.pathname === '/upload') {
        const chunks: Buffer[] = []
        req.on('data', (chunk: Buffer) => { chunks.push(chunk) })
        req.on('end', () => {
          const rawBody = Buffer.concat(chunks).toString('utf-8')
          res.writeHead(200, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify({
            contentType: req.headers['content-type'] ?? null,
            contentLength: req.headers['content-length'] ?? null,
            bodyPreview: rawBody,
          }))
        })
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

beforeEach(async () => {
  await startServer()
  const addr = server.address() as any
  baseUrl = `http://127.0.0.1:${addr.port}`
})

afterEach(async () => {
  await stopServer()
})

// ── F1-F5: FormData / multipart ───────────────────────────────────

describe('F1 — auto-detect FormData body', () => {
  it('Content-Type includes multipart/form-data', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const form = new FormData()
    form.append('field', 'value')

    const data = await client.post('/upload', form)
    expect(data.contentType).toContain('multipart/form-data')
  })
})

describe('F2 — fields sent correctly', () => {
  it('body contains field name and value', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const form = new FormData()
    form.append('username', 'alice')

    const data = await client.post('/upload', form)
    expect(data.bodyPreview).toContain('name="username"')
    expect(data.bodyPreview).toContain('alice')
  })
})

describe('F3 — file upload', () => {
  it('body contains filename and file content', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const form = new FormData()
    form.append('file', new Blob(['hello world'], { type: 'text/plain' }), 'test.txt')

    const data = await client.post('/upload', form)
    expect(data.bodyPreview).toContain('filename="test.txt"')
    expect(data.bodyPreview).toContain('hello world')
  })
})

describe('F4 — multi-file upload', () => {
  it('body contains both filenames', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const form = new FormData()
    form.append('files', new Blob(['a'], { type: 'text/plain' }), 'file1.txt')
    form.append('files', new Blob(['b'], { type: 'text/plain' }), 'file2.txt')

    const data = await client.post('/upload', form)
    expect(data.bodyPreview).toContain('filename="file1.txt"')
    expect(data.bodyPreview).toContain('filename="file2.txt"')
  })
})

describe('F5 — mixed fields and files', () => {
  it('body contains field name and filename', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const form = new FormData()
    form.append('name', 'test')
    form.append('file', new Blob(['data'], { type: 'text/plain' }), 'data.csv')

    const data = await client.post('/upload', form)
    expect(data.bodyPreview).toContain('name="name"')
    expect(data.bodyPreview).toContain('filename="data.csv"')
  })
})
