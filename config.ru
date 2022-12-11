require 'syslog-exporter/rackup'

run SyslogExporter::Rackup.app(ENV['SYSLOG_EXPORTER_CONFIG'] || 'config-sample.json')
