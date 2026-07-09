RSpec.describe SyslogExporter::Config do
  let(:config_path) do
    file = Tempfile.new('syslog-exporter-config')
    file.write(JSON.generate(config))
    file.close
    file.path
  end

  let(:config) do
    {
      'syslog_pipe' => '/run/syslog.pipe',
      'hosts' => {
        'cz.vpsfree/nodes/prg/node1' => {
          'alias' => 'node1.prg',
          'fqdn' => 'node1.prg.vpsfree.cz',
          'os' => 'vpsadminos'
        }
      }
    }
  end

  after do
    File.unlink(config_path)
  end

  it 'loads pipe and host settings' do
    cfg = described_class.new(config_path)
    host = cfg.hosts.fetch('cz.vpsfree/nodes/prg/node1')

    expect(cfg.syslog_pipe).to eq('/run/syslog.pipe')
    expect(cfg.pipe_size).to eq(1 * 1024 * 1024)
    expect(host.alias_name).to eq('node1.prg')
    expect(host.fqdn).to eq('node1.prg.vpsfree.cz')
    expect(host.os).to eq('vpsadminos')
    expect(host.collectors).to eq([])
  end

  it 'honors explicit pipe size' do
    config['pipe_size'] = 256 * 1024

    expect(described_class.new(config_path).pipe_size).to eq(256 * 1024)
  end

  it 'loads configured host collectors' do
    config['hosts']['cz.vpsfree/nodes/prg/node1']['collectors'] = ['vpsadmin_webui']

    host = described_class.new(config_path).hosts.fetch('cz.vpsfree/nodes/prg/node1')

    expect(host.collectors).to eq(['vpsadmin_webui'])
  end
end
