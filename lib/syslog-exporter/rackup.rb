require 'syslog-exporter'
require 'rack'
require 'prometheus/middleware/exporter'

module SyslogExporter
  module Rackup
    def self.app(config_file)
      Thread.abort_on_exception = true

      registry = SyslogExporter.registry

      processor = SyslogExporter::Processor.new(
        Config.new(config_file),
        registry
      )
      processor.start

      Rack::Builder.app do
        use Rack::Deflater
        use Prometheus::Middleware::Exporter, { registry: }

        run ->(_) { [200, { 'Content-Type' => 'text/html' }, ['OK']] }
      end
    end
  end
end
