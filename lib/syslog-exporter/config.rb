require 'json'

module SyslogExporter
  # syslog-exporter's config
  class Config
    class Host
      # @return [String]
      attr_reader :name

      # @return [String]
      attr_reader :alias_name

      # @return [String]
      attr_reader :fqdn

      # @return [String]
      attr_reader :os

      def initialize(name:, alias_name:, fqdn:, os:)
        @name = name
        @alias_name = alias_name
        @fqdn = fqdn
        @os = os
      end
    end

    # @return [String]
    attr_reader :syslog_pipe

    # @return [Integer]
    attr_reader :pipe_size

    # @return [Hash<String, Host>]
    attr_reader :hosts

    def initialize(path)
      data = JSON.parse(File.read(path))

      @syslog_pipe = data['syslog_pipe']
      @pipe_size = data.fetch('pipe_size', 1*1024*1024)
      @hosts = Hash[data['hosts'].map do |k, v|
        h = Host.new(
          name: k,
          alias_name: v['alias'],
          fqdn: v['fqdn'],
          os: v['os'],
        )
        [h.name, h]
      end]
    end
  end
end
