lib = File.expand_path('lib', __dir__)
$:.unshift(lib) unless $:.include?(lib)
require 'syslog-exporter/version'

Gem::Specification.new do |s|
  s.name          = 'syslog-exporter'
  s.version       = SyslogExporter::VERSION

  s.summary       =
    s.description = 'Generate metrics from syslog for node_exporter'
  s.authors       = 'Jakub Skokan'
  s.email         = 'jakub.skokan@vpsfree.cz'
  s.files         = `git ls-files -z`.split("\x0")
  s.executables   = s.files.grep(%r{^bin/}) { |f| File.basename(f) }
  s.require_paths = ['lib']
  s.license       = 'MIT'

  s.required_ruby_version = '>= 3.1.0'

  s.add_dependency 'prometheus-client', '~> 4.2.2'
  s.add_dependency 'puma', '~> 6.4'
  s.add_dependency 'rack', '~> 3.1'
end
