module SyslogExporter
  class Collectors::VpsadminWebui < Collector
    register :vpsadmin_webui

    ERROR_TYPES = {
      'fatal' => 'PHP message: PHP Fatal error:',
      'parse' => 'PHP message: PHP Parse error:',
      'recoverable_fatal' => 'PHP message: PHP Recoverable fatal error:'
    }.freeze

    def self.setup(registry)
      add_metric(
        registry,
        :counter,
        :syslog_vpsadmin_webui_fatal_error_count,
        docstring: 'Number of fatal vpsAdmin WebUI PHP errors reported by nginx',
        labels: %i[error_type]
      )
      add_metric(
        registry,
        :gauge,
        :syslog_vpsadmin_webui_fatal_error,
        docstring: '1 if a fatal vpsAdmin WebUI PHP error has occurred, 0 otherwise',
        labels: %i[error_type]
      )
    end

    def self.use_host?(host)
      host.collectors.include?('vpsadmin_webui')
    end

    def setup
      ERROR_TYPES.each_key do |error_type|
        set_gauge(:syslog_vpsadmin_webui_fatal_error, 0, labels: { error_type: })
      end
    end

    def <<(message)
      return unless message.program == 'nginx'
      return unless message.message.include?('FastCGI sent in stderr')

      error_type = detect_error_type(message.message)
      return if error_type.nil?

      labels = { error_type: }

      increment_counter(:syslog_vpsadmin_webui_fatal_error_count, labels:)
      set_flare(:syslog_vpsadmin_webui_fatal_error, 1, labels:, seconds: 180)
    end

    protected

    def detect_error_type(message)
      ERROR_TYPES.detect { |_type, signature| message.include?(signature) }&.first
    end
  end
end
