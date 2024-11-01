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
      # lxc-start was there originally, since perhaps 6.0.2 it is lxc
      return unless %w[lxc lxc-start].include?(message.program)

      if message.message.include?('No space left on device - Failed to unshare CLONE_NEWNET')
        set_flare(
          :syslog_lxc_start_netns_limit,
          1,
          labels: { id: get_container_id(message) },
          seconds: 240
        )
      elsif /Failed to spawn container "[^"]+"/ =~ message.message
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
      # 1234: start - ../src/lxc/start.c:do_start:1108 - No space left on device - Failed to unshare CLONE_NEWNET
      # 1234: start - ../src/lxc/start.c:__lxc_start:2120 - Failed to spawn container "26533"
      # where 1234 is container id
      colon = message.message.index(':')
      return '' if colon.nil?

      message.message[0..(colon - 1)].strip
    end
  end
end
