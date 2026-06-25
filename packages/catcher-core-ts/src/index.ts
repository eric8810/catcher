export type {
  SharedAgentOptions,
  HttpClientConfig,
  IHttpClient,
  ResilientWSOptions,
  ResilientWS,
  PriorityQueueOptions,
  RetryOptions,
  RequestConfig,
  ProgressEvent,
  HttpResponse,
  InterceptorFulfilled,
  InterceptorRejected,
  InterceptorHandler,
  InterceptorManager,
  SSEStreamOptions,
  SSEClientOptions,
  SSEStream,
  SSEClient,
  SSETimeoutError,
  // G2: Error types
  CatcherErrorType,
  CatcherHttpError,
  // G3: CORS
  ProxyConfig,
  DnsConfig,
  TlsConfig,
  RedirectInfo,
  // G9: Transport
  TransportAdapter,
  // G11: Events
  ClientEvent,
} from './types.js'

export { isCatcherError } from './types.js'
export { createInterceptorManager } from './interceptors.js'
export { routeLine, PushQueue } from './sse-router.js'
export type { RouteAction } from './sse-router.js'
export { createExecutor, sleep, calculateDelay, SENSITIVE_HEADERS, classifyFetchError, redactHeaders, SSETimeoutErrorImpl, createSSEStreamCore, createSSEClientCore } from './helpers.js'
export type { SseConnectOnceCtx, ConnectOnceFn } from './helpers.js'
