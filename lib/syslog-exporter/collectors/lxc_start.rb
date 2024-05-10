module SyslogExporter
  class Collectors::LxcStart < Collector
    register :lxc_start

    def self.setup(registry)
      add_metric(
        registry,
        :gauge,
        :syslog_lxc_start_failed,
        docstring: '1 if a container has failed to start, 0 otherwise',
        labels: %i[id]
      )
      add_metric(
        registry,
        :gauge,
        :syslog_lxc_start_netns_limit,
        docstring: '1 if a container has hit netns limit and failed to start, 0 otherwise',
        labels: %i[id]
      )
    end

    def self.use_host?(host)
      host.os == 'vpsadminos'
    end

    def <<(message)
      return if message.program != 'lxc-start'

      if message.message.include?('No space left on device - Failed to unshare CLONE_NEWNET')
        set_flare(
          :syslog_lxc_start_netns_limit,
          1,
          labels: { id: get_container_id(message) },
          seconds: 240
        )
      elsif message.message.include?('The container failed to start')
        set_flare(
          :syslog_lxc_start_failed,
          1,
          labels: { id: get_container_id(message) },
          seconds: 240
        )
      end
    end

    protected

    def get_container_id(message)
      colon = message.message.index(':')
      return '' if colon.nil?

      message.message[0..(colon - 1)].strip
    end
  end
end
