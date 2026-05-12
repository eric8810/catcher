export class WsClient {
  constructor(configJson: string, onEvent?: (eventJson: string) => void)
  send(data: string): void
  close(): void
}
