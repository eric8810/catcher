Pod::Spec.new do |s|
  s.name             = 'catcher_core'
  s.version          = '0.3.12'
  s.summary          = 'Resilient HTTP/WebSocket client backed by Rust core for Flutter.'
  s.description      = <<-DESC
Resilient HTTP/WebSocket client backed by Rust core for Flutter.
                       DESC
  s.homepage         = 'https://github.com/eric8810/catcher'
  s.license          = { :file => '../LICENSE' }
  s.author           = { 'catcher' => 'https://github.com/eric8810/catcher' }
  s.source           = { :path => '.' }

  s.dependency 'Flutter'
  s.platform = :ios, '15.0'
  s.vendored_frameworks = 'Frameworks/catcher_ffi.xcframework'
  s.preserve_paths = 'Frameworks/catcher_ffi.xcframework'
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386'
  }
  s.swift_version = '5.0'
end
