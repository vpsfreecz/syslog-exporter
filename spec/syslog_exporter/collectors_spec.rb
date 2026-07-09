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

  it 'detects vpsAdmin WebUI fatal errors reported by nginx' do
    collector, registry = setup_collector(SyslogExporter::Collectors::VpsadminWebui, host: webui_host)

    collector << message(
      program: 'nginx',
      message: 'FastCGI sent in stderr: "PHP message: PHP Warning: noise; ' \
               'PHP message: PHP Fatal error:  Uncaught TypeError: broken"',
      host: 'webui1.int.vpsfree.cz'
    )

    expect(registry.metrics.fetch(:syslog_vpsadmin_webui_fatal_error_count).increments.last).to eq(
      labels: {
        error_type: 'fatal',
        alias: 'webui1.int',
        fqdn: 'webui1.int.vpsfree.cz'
      }
    )
    expect(registry.metrics.fetch(:syslog_vpsadmin_webui_fatal_error).sets.last).to eq(
      value: 1,
      labels: {
        error_type: 'fatal',
        alias: 'webui1.int',
        fqdn: 'webui1.int.vpsfree.cz'
      }
    )
  end

  it 'classifies vpsAdmin WebUI parse and recoverable fatal errors' do
    collector, registry = setup_collector(SyslogExporter::Collectors::VpsadminWebui, host: webui_host)

    collector << message(
      program: 'nginx',
      message: 'FastCGI sent in stderr: "PHP message: PHP Parse error: syntax error"',
      host: 'webui1.int.vpsfree.cz'
    )
    collector << message(
      program: 'nginx',
      message: 'FastCGI sent in stderr: "PHP message: PHP Recoverable fatal error: problem"',
      host: 'webui1.int.vpsfree.cz'
    )

    expect(registry.metrics.fetch(:syslog_vpsadmin_webui_fatal_error_count).increments).to include(
      labels: {
        error_type: 'parse',
        alias: 'webui1.int',
        fqdn: 'webui1.int.vpsfree.cz'
      }
    )
    expect(registry.metrics.fetch(:syslog_vpsadmin_webui_fatal_error_count).increments).to include(
      labels: {
        error_type: 'recoverable_fatal',
        alias: 'webui1.int',
        fqdn: 'webui1.int.vpsfree.cz'
      }
    )
  end

  it 'ignores WebUI warnings, stack traces, handled errors, and other programs' do
    collector, registry = setup_collector(SyslogExporter::Collectors::VpsadminWebui, host: webui_host)

    [
      message(
        program: 'nginx',
        message: 'FastCGI sent in stderr: "PHP message: PHP Warning: Undefined array key"',
        host: 'webui1.int.vpsfree.cz'
      ),
      message(program: 'nginx', message: 'Stack trace:', host: 'webui1.int.vpsfree.cz'),
      message(
        program: 'nginx',
        message: 'FastCGI sent in stderr: "PHP message: HaveAPI\Client\Exception\ValidationError: invalid"',
        host: 'webui1.int.vpsfree.cz'
      ),
      message(
        program: 'php-fpm',
        message: 'FastCGI sent in stderr: "PHP message: PHP Fatal error: bad"',
        host: 'webui1.int.vpsfree.cz'
      )
    ].each { |msg| collector << msg }

    expect(registry.metrics.fetch(:syslog_vpsadmin_webui_fatal_error_count).increments).to be_empty
    expect(registry.metrics.fetch(:syslog_vpsadmin_webui_fatal_error).sets.map { |set| set[:value] }).to all(eq(0))
  end

  it 'uses the vpsAdmin WebUI collector only for configured hosts' do
    expect(SyslogExporter::Collectors::VpsadminWebui.use_host?(webui_host)).to be(true)
    expect(SyslogExporter::Collectors::VpsadminWebui.use_host?(default_host)).to be(false)
  end
end
