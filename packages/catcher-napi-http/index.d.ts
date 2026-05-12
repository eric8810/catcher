export class JsHttpResponse {
  status: number
  headers: Record<string, string>
  body: Buffer
  elapsedMs: number
}

export class HttpClient {
  constructor(configJson: string)
  get(url: string): Promise<JsHttpResponse>
  post(url: string, body: Buffer, contentType?: string): Promise<JsHttpResponse>
}
