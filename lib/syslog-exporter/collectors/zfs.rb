module SyslogExporter
  class Collectors::Zfs < Collector
    register :zfs

    def self.setup(registry)
      add_metric(
        registry,
        :gauge,
        :syslog_zfs_panic,
        docstring: '1 if ZFS panic has occurred, 0 otherwise',
      )
    end

    def setup
      set_gauge(:syslog_zfs_panic, 0)
    end

    def <<(message)
      return if message.program != 'kernel'

      if message.message.include?('PANIC: zfs:')
        set_flare(:syslog_zfs_panic, 1, seconds: 240)
      end
    end
  end
end
