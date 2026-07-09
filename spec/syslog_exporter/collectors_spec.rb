RSpec.describe 'syslog collectors' do
  it 'counts messages by program' do
    collector, registry = setup_collector(SyslogExporter::Collectors::MessageCount)

    collector << message(program: 'nginx', message: 'request failed')

    metric = registry.metrics.fetch(:syslog_message_count)
    expect(metric.increments).to contain_exactly(
      labels: {
        program: 'nginx',
        alias: 'node1.prg',
        fqdn: 'node1.prg.vpsfree.cz'
      }
    )
  end

  it 'detects nodectld crashes' do
    collector, registry = setup_collector(SyslogExporter::Collectors::Nodectld)

    collector << message(program: 'nodectld', message: 'Daemon crashed')

    metric = registry.metrics.fetch(:syslog_nodectld_crash)
    expect(metric.sets.last).to eq(
      value: 1,
      labels: {
        alias: 'node1.prg',
        fqdn: 'node1.prg.vpsfree.cz'
      }
    )
  end

  it 'detects osctld commands and internal errors' do
    collector, registry = setup_collector(SyslogExporter::Collectors::Osctld)

    collector << message(program: 'osctld', message: "Received command 'ct.start'")
    collector << message(program: 'osctld', message: 'Error: internal error')

    expect(registry.metrics.fetch(:syslog_osctld_command_count).increments).to include(
      labels: {
        command: 'ct.start',
        alias: 'node1.prg',
        fqdn: 'node1.prg.vpsfree.cz'
      }
    )
    expect(registry.metrics.fetch(:syslog_osctld_internal_error).sets.last[:value]).to eq(1)
  end

  it 'detects lxc start failures and extracts container ids' do
    collector, registry = setup_collector(SyslogExporter::Collectors::LxcStart)

    collector << message(
      program: 'lxc',
      message: '26533: start - ../src/lxc/start.c:__lxc_start:2120 - Failed to spawn container "26533"'
    )

    expect(registry.metrics.fetch(:syslog_lxc_start_failed).sets.last).to eq(
      value: 1,
      labels: {
        id: '26533',
        alias: 'node1.prg',
        fqdn: 'node1.prg.vpsfree.cz'
      }
    )
  end

  it 'tracks kernel oom messages with namespace labels' do
    collector, registry = setup_collector(SyslogExporter::Collectors::Kernel)
    kernel_message = SyslogExporter::KernelMessage.new(
      time: Time.utc(2026, 7, 9, 12, 0, 0),
      host: 'node1.prg.vpsfree.cz',
      program: 'kernel',
      message: '[ prg ] apache2 invoked oom-killer: gfp_mask=0x100cca'
    )

    collector << kernel_message

    expect(registry.metrics.fetch(:syslog_kernel_message_count).increments).to include(
      labels: {
        syslogns: 'prg',
        alias: 'node1.prg',
        fqdn: 'node1.prg.vpsfree.cz'
      }
    )
    expect(registry.metrics.fetch(:syslog_kernel_oom_count).increments.last[:labels]).to include(
      alias: 'node1.prg',
      fqdn: 'node1.prg.vpsfree.cz'
    )
  end

  it 'resets expired flare gauges' do
    collector, registry = setup_collector(SyslogExporter::Collectors::Zfs)

    collector << message(program: 'kernel', message: 'PANIC: zfs: bad state')
    flare = collector.instance_variable_get(:@flares).first
    flare.instance_variable_set(:@ends_at, Time.now - 1)
    collector.settle_flares

    expect(registry.metrics.fetch(:syslog_zfs_panic).sets.last).to eq(
      value: 0,
      labels: {
        alias: 'node1.prg',
        fqdn: 'node1.prg.vpsfree.cz'
      }
    )
  end
end
