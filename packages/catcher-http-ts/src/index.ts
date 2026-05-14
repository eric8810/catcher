export { createHttpClient, createRetryWrapper, createInterceptorManager, classifyAxiosError, classifyFetchError, createCatcherError } from './http/index.js'
export { createSharedAgent, clearDnsCache } from './agent/index.js'
export { createPriorityQueue, enqueueWithPriority } from './queue/index.js'
export { createSSEStream, createSSEClient } from './sse/index.js'

export type {
  IHttpClient,
  RequestConfig,
  ProgressEvent,
  HttpResponse,
  InterceptorManager,
  InterceptorHandler,
  InterceptorFulfilled,
  InterceptorRejected,
  SSEStreamOptions,
  SSEClientOptions,
  SSEStream,
  SSEClient,
  SSETimeoutError,
  CatcherHttpError,
  CatcherErrorType,
  ProxyConfig,
  DnsConfig,
  TlsConfig,
  RedirectInfo,
  TransportAdapter,
  ClientEvent,
} from '@eric8810/catcher-core'

export { isCatcherError } from '@eric8810/catcher-core'