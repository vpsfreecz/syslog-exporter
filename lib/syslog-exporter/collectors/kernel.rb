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
        :syslog_kernel_emergency,
        docstring: '1 if kernel emergency message has been detected, 0 otherwise',
        labels: %i(type),
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
      end
    end
  end
end
