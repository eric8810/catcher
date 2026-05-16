import type { CatcherErrorType, CatcherHttpError, RequestConfig } from '@eric8810/catcher-core'

const SENSITIVE_HEADERS = new Set(['authorization', 'cookie', 'set-cookie', 'proxy-authorization'])

/**
 * Classify an axios error into a CatcherErrorType.
 */
export function classifyAxiosError(error: any): CatcherErrorType {
  if (error.code === 'ECONNABORTED' || error.code === 'ETIMEDOUT') return 'timeout'
  if (error.code === 'ECONNREFUSED') return 'connection'
  if (error.code === 'ENOTFOUND') return 'dns'
  if (
    error.code === 'UNABLE_TO_VERIFY_LEAF_SIGNATURE' ||
    error.code === 'CERT_HAS_EXPIRED' ||
    error.code === 'DEPTH_ZERO_SELF_SIGNED_CERT' ||
    error.code === 'ERR_TLS_CERT_ALTNAME_INVALID'
  ) return 'tls'
  if (error.name === 'CanceledError' || error.code === 'ERR_CANCELED' || error.code === 'ECANCELED') return 'cancelled'
  if (error.response) return 'http'
  return 'unknown'
}

/**
 * Classify a browser fetch error into a CatcherErrorType.
 */
export function classifyFetchError(error: any): CatcherErrorType {
  if (error.name === 'AbortError' || error.code === 'ECANCELED') return 'cancelled'
  if (error.name === 'TypeError' && error.message?.includes('Failed to fetch')) return 'connection'
  if (error.code === 'HTTP_5XX') return 'http'
  if (error.response) return 'http'
  return 'unknown'
}

/**
 * Redact sensitive headers for safe serialization.
 */
function redactHeaders(headers: Record<string, string>): Record<string, string> {
  const safe: Record<string, string> = {}
  for (const [key, value] of Object.entries(headers)) {
    safe[key] = SENSITIVE_HEADERS.has(key.toLowerCase()) ? '[REDACTED]' : value
  }
  return safe
}

/**
 * Convert response data to a Uint8Array for rawData field.
 */
function toRawData(data: unknown): Uint8Array | undefined {
  if (data == null) return undefined
  if (data instanceof ArrayBuffer) return new Uint8Array(data)
  if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
  if (typeof data === 'string') return new TextEncoder().encode(data)
  try { return new TextEncoder().encode(JSON.stringify(data)) } catch { return undefined }
}

/**
 * Create a CatcherHttpError from an underlying error.
 */
export function createCatcherError(
  error: any,
  type: CatcherErrorType,
  method: string,
  url: string,
  headers: Record<string, string>,
  config: RequestConfig,
  attempt: number,
  elapsedMs: number,
): CatcherHttpError {
  const err = new Error(error.message ?? String(error)) as Error & CatcherHttpError
  err.name = 'CatcherHttpError'

  ;(err as any).type = type
  ;(err as any).request = {
    method,
    url,
    headers,
    config,
  }

  if (error.response) {
    ;(err as any).response = {
      status: error.response.status,
      headers: error.response.headers ?? {},
      data: error.response.data,
      rawData: error.response.rawData ?? toRawData(error.response.data),
    }
  }

  ;(err as any).attempt = attempt
  ;(err as any).elapsedMs = elapsedMs
  ;(err as any).toJSON = () => ({
    type,
    message: err.message,
    request: {
      method,
      url,
      headers: redactHeaders(headers),
    },
    response: (err as any).response
      ? { status: (err as any).response.status, data: (err as any).response.data }
      : undefined,
    attempt,
    elapsedMs,
  })

  // Preserve original stack
  if (error.stack) err.stack = error.stack

  return err as unknown as CatcherHttpError
}
