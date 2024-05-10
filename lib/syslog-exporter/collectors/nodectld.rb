module SyslogExporter
  class Collectors::Nodectld < Collector
    register :nodectld

    def self.setup(registry)
      add_metric(
        registry,
        :gauge,
        :syslog_nodectld_crash,
        docstring: '1 if nodectld has crashed, 0 otherwise'
      )
      add_metric(
        registry,
        :gauge,
        :syslog_nodectld_segfault,
        docstring: '1 if nodectld has segfaulted, 0 otherwise'
      )
      add_metric(
        registry,
        :gauge,
        :syslog_nodectld_zfs_stream_receive_error,
        docstring: '1 if zfs recv in nodectld has reported an error, 0 otherwise'
      )
    end

    def self.use_host?(host)
      host.os == 'vpsadminos'
    end

    def setup
      set_gauge(:syslog_nodectld_crash, 0)
      set_gauge(:syslog_nodectld_segfault, 0)
      set_gauge(:syslog_nodectld_zfs_stream_receive_error, 0)
    end

    def <<(message)
      return if message.program != 'nodectld'

      if message.message.include?('Daemon crashed')
        set_flare(:syslog_nodectld_crash, 1, seconds: 180)
      elsif message.message.include?('[BUG] Segmentation fault')
        set_flare(:syslog_nodectld_segfault, 1, seconds: 180)
      elsif message.message.include?('cannot receive incremental stream')
        set_flare(:syslog_nodectld_zfs_stream_receive_error, 1, seconds: 180)
      end
    end
  end
end
