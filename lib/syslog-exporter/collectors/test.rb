module SyslogExporter
  class Collectors::Test < Collector
    register :test

    def self.setup(registry)
      add_metric(
        registry,
        :gauge,
        :syslog_flare_test,
        docstring: '1 if flare is on, 0 otherwise',
      )
    end

    def setup
      set_gauge(:syslog_flare_test, 0)
    end

    def <<(message)
      if message.message.include?('flare test')
        set_flare(:syslog_flare_test, 1, seconds: 15)
      end
    end
  end
end
