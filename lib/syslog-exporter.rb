require 'prometheus/client'

module SyslogExporter
  def self.registry
    @registry ||= Prometheus::Client::Registry.new
  end
end

require_relative 'syslog-exporter/config'
require_relative 'syslog-exporter/collector'
require_relative 'syslog-exporter/collectors'
require_relative 'syslog-exporter/flare'
require_relative 'syslog-exporter/message'
require_relative 'syslog-exporter/parser'
require_relative 'syslog-exporter/processor'
require_relative 'syslog-exporter/version'
require_relative 'syslog-exporter/collectors/kernel'
require_relative 'syslog-exporter/collectors/lxc_start'
require_relative 'syslog-exporter/collectors/message_count'
require_relative 'syslog-exporter/collectors/nodectld'
require_relative 'syslog-exporter/collectors/osctld'
require_relative 'syslog-exporter/collectors/test'
require_relative 'syslog-exporter/collectors/zfs'
