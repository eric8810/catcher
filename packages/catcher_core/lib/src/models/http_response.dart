/// HTTP 响应
class HttpResponse {
  final int status;
  final Map<String, String> headers;
  final List<int> body;
  final int elapsedMs;

  const HttpResponse({
    required this.status,
    this.headers = const {},
    this.body = const [],
    this.elapsedMs = 0,
  });

  factory HttpResponse.fromJson(Map<String, dynamic> json) => HttpResponse(
    status: json['status'] as int,
    headers: (json['headers'] as Map<String, dynamic>?)?.map(
      (k, v) => MapEntry(k, v.toString()),
    ) ?? {},
    body: (json['body'] as List<dynamic>?)?.cast<int>() ?? [],
    elapsedMs: json['elapsed_ms'] as int? ?? 0,
  );

  String get bodyAsString => String.fromCharCodes(body);
}
