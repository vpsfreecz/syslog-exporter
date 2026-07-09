RSpec.describe SyslogExporter::Parser do
  def parse(line)
    described_class.new(StringIO.new).send(:parse_message, line)
  end

  it 'parses program names and pids from rsyslog messages' do
    msg = parse('2026-07-09T12:00:00+00:00 node1.prg.vpsfree.cz osctld[123] Started')

    expect(msg.time).to eq(Time.iso8601('2026-07-09T12:00:00+00:00'))
    expect(msg.host).to eq('node1.prg.vpsfree.cz')
    expect(msg.program).to eq('osctld')
    expect(msg.pid).to eq(123)
    expect(msg.message).to eq('Started')
  end

  it 'creates kernel messages for kernel program names' do
    msg = parse('2026-07-09T12:00:00+00:00 node1.prg.vpsfree.cz kernel[] [ prg ] invoked oom-killer')

    expect(msg).to be_a(SyslogExporter::KernelMessage)
    expect(msg.program).to eq('kernel')
    expect(msg.syslogns_tag).to eq('prg')
  end

  it 'parses svlogd-forwarded messages from localhost' do
    msg = parse(
      '2026-07-09T12:00:00+00:00 localhost ' \
      '2026-07-09_12 00:00.000000 node1.prg.vpsfree.cz nodectld Daemon crashed'
    )

    expect(msg.host).to eq('node1.prg.vpsfree.cz')
    expect(msg.program).to eq('nodectld')
    expect(msg.message).to eq('Daemon crashed')
  end

  it 'drops the first line and skips malformed messages while iterating' do
    input = StringIO.new(<<~LOG)
      incomplete
      malformed
      2026-07-09T12:00:00+00:00 node1.prg.vpsfree.cz osctld[123] Started
    LOG

    messages = []

    expect do
      described_class.new(input).each_message { |msg| messages << msg }
    end.to output(/Unknown message/).to_stderr

    expect(messages.size).to eq(1)
    expect(messages.first.program).to eq('osctld')
  end
end
