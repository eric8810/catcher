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
