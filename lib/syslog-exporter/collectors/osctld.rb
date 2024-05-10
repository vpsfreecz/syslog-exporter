module SyslogExporter
  class Collectors::Osctld < Collector
    register :osctld

    def self.setup(registry)
      add_metric(
        registry,
        :counter,
        :syslog_osctld_command_count,
        docstring: 'Number of executed management commands',
        labels: %i[command]
      )
      add_metric(
        registry,
        :gauge,
        :syslog_osctld_internal_error,
        docstring: '1 if osctld internal error has occurred, 0 otherwise'
      )
    end

    def self.use_host?(host)
      host.os == 'vpsadminos'
    end

    def setup
      set_gauge(:syslog_osctld_internal_error, 0)
    end

    def <<(message)
      return if message.program != 'osctld'

      if /Received command '([^']+)'$/ =~ message.message
        increment_counter(:syslog_osctld_command_count, labels: { command: ::Regexp.last_match(1) })
      elsif message.message.include?('Error: internal error')
        set_flare(:syslog_osctld_internal_error, 1, seconds: 180)
      end
    end
  end
end
