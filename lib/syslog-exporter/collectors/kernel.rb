module SyslogExporter
  class Collectors::Kernel < Collector
    register :kernel

    BUG_TYPES = %w(nullptr)

    EMERGENCY_TYPES = %w(unregister_netdevice)

    def self.setup(registry)
      add_metric(
        registry,
        :gauge,
        :syslog_kernel_bug,
        docstring: '1 if kernel bug has occurred, 0 otherwise',
        labels: %i(type),
      )
      add_metric(
        registry,
        :gauge,
        :syslog_kernel_gpf,
        docstring: '1 if kernel general protection fault has occurred, 0 otherwise',
      )
      add_metric(
        registry,
        :gauge,
        :syslog_kernel_emergency,
        docstring: '1 if kernel emergency message has been detected, 0 otherwise',
        labels: %i(type),
      )
      add_metric(
        registry,
        :gauge,
        :syslog_kernel_nf_conntrack_table_full,
        docstring: '1 if kernel nf_conntrack table full message has been detected, 0 otherwise',
      )
    end

    def setup
      BUG_TYPES.each do |v|
        set_gauge(:syslog_kernel_bug, 0, labels: {type: v})
      end

      EMERGENCY_TYPES.each do |v|
        set_gauge(:syslog_kernel_emergency, 0, labels: {type: v})
      end
    end

    def <<(message)
      return if message.program != 'kernel'

      if message.message.include?('BUG: kernel NULL pointer dereference')
        set_flare(:syslog_kernel_bug, 1, labels: {type: 'nullptr'}, seconds: 240)

      # unregister_netdevice: waiting for lo to become free. Usage count = 1
      elsif message.message.include?('unregister_netdevice: waiting for')
        set_flare(:syslog_kernel_emergency, 1, labels: {type: 'unregister_netdevice'}, seconds: 240)

      # catch:  [26764.620512] general protection fault, probably for non-canonical address 0xdead000000000100: 0000 [#1] PREEMPT SMP PTI
      # ignore: [222090.965241] traps: dotnet[1566152] general protection fault ip:7f3ce0220611 sp:7ffe37ee4f40 error:0 in libc-2.28.so[7f3ce0220000+148000]
      elsif message.message.include?('general protection fault') && !message.message.include?('traps: ')
        set_flare(:syslog_kernel_gpf, 1, seconds: 240)

      # nf_conntrack: nf_conntrack: table full, dropping packet
      elsif message.message.include?('nf_conntrack: nf_conntrack: table full, dropping packet')
        set_flare(:syslog_kernel_nf_conntrack_table_full, 1, seconds: 240)
      end
    end
  end
end
