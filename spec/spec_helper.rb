$:.unshift(File.expand_path('../lib', __dir__))

require 'json'
require 'stringio'
require 'tempfile'

require 'syslog-exporter'

module MetricHelpers
  class Metric
    attr_reader :type, :name, :docstring, :labels, :sets, :increments

    def initialize(type:, name:, docstring:, labels:)
      @type = type
      @name = name
      @docstring = docstring
      @labels = labels
      @sets = []
      @increments = []
    end

    def set(value, labels:)
      @sets << { value: value, labels: labels }
    end

    def increment(labels:)
      @increments << { labels: labels }
    end
  end

  class Registry
    attr_reader :metrics

    def initialize
      @metrics = {}
    end

    def gauge(name, docstring:, labels: [])
      add(:gauge, name, docstring:, labels:)
    end

    def counter(name, docstring:, labels: [])
      add(:counter, name, docstring:, labels:)
    end

    private

    def add(type, name, docstring:, labels:)
      @metrics[name] = Metric.new(type:, name:, docstring:, labels:)
    end
  end

  def setup_collector(klass, host: default_host)
    registry = Registry.new
    klass.setup(registry)
    [klass.new(host), registry]
  end

  def default_host
    SyslogExporter::Config::Host.new(
      name: 'cz.vpsfree/nodes/prg/node1',
      alias_name: 'node1.prg',
      fqdn: 'node1.prg.vpsfree.cz',
      os: 'vpsadminos'
    )
  end

  def message(program:, message:, host: 'node1.prg.vpsfree.cz')
    SyslogExporter::Message.new(
      time: Time.utc(2026, 7, 9, 12, 0, 0),
      host: host,
      program: program,
      message: message
    )
  end
end

RSpec.configure do |config|
  config.include MetricHelpers
  config.disable_monkey_patching!
  config.expect_with(:rspec) do |expectations|
    expectations.syntax = :expect
  end
end
