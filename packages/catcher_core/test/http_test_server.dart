import 'dart:convert';
import 'dart:io';

class LocalHttpEchoServer {
  LocalHttpEchoServer._(this._server, this.baseUrl);

  final HttpServer _server;
  final String baseUrl;

  static Future<LocalHttpEchoServer> start() async {
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    server.listen(_handleRequest);
    return LocalHttpEchoServer._(
      server,
      'http://127.0.0.1:${server.port}',
    );
  }

  Future<void> close() async {
    await _server.close(force: true);
  }

  static Future<void> _handleRequest(HttpRequest request) async {
    final path = request.uri.path;

    if (request.method == 'GET' && path == '/get') {
      await _writeJson(request.response, 200, {
        'url': request.requestedUri.toString(),
      });
      return;
    }

    if (request.method == 'GET' && path == '/status/404') {
      await _writeJson(request.response, 404, {
        'status': 404,
      });
      return;
    }

    if (request.method == 'POST' && path == '/post') {
      final data = await utf8.decoder.bind(request).join();
      await _writeJson(request.response, 200, {
        'data': data,
      });
      return;
    }

    if (request.method == 'GET' && path == '/headers') {
      final headers = <String, String>{};
      request.headers.forEach((name, values) {
        headers[name] = values.join(', ');
      });
      await _writeJson(request.response, 200, {
        'headers': headers,
      });
      return;
    }

    await _writeJson(request.response, 404, {
      'error': 'not found',
    });
  }

  static Future<void> _writeJson(
    HttpResponse response,
    int statusCode,
    Object body,
  ) async {
    response.statusCode = statusCode;
    response.headers.contentType = ContentType.json;
    response.write(jsonEncode(body));
    await response.close();
  }
}
