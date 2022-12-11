module SyslogExporter
  class Collectors::MessageCount < Collector
    register :message_count

    def self.setup(registry)
      add_metric(
        registry,
        :counter,
        :syslog_message_count,
        docstring: 'Number of syslog messages sent by a host',
        labels: %i(program),
      )
    end

    def <<(message)
      increment_counter(:syslog_message_count, labels: {
        program: message.program || 'other',
      })
    end
  end
end
