require 'thread'

module SyslogExporter
  class Processor
    def initialize(config, registry)
      @config = config
      @registry = registry
      @io = open_pipe(config)
      @queue = Queue.new
      @hosts_collectors = {}

      Collectors.each do |_, collector_class|
        collector_class.setup(registry)

        config.hosts.each do |name, host|
          next unless collector_class.use_host?(host)

          @hosts_collectors[name] ||= []
          @hosts_collectors[name] << collector_class.new(host)
        end
      end
    end

    def start
      @parser_thread = Thread.new { run_parser }
      @process_thread = Thread.new { run_processor }
      @flare_thread = Thread.new { run_flare_settler }
    end

    protected
    def run_parser
      parser = Parser.new(@io)
      parser.each_message { |msg| @queue << msg }
    end

    def run_processor
      loop do
        msg = @queue.pop
        break if msg == :eof

        # Find a matching host, either by name or fqdn
        host = @config.hosts.each_value.detect do |h|
          h.name == msg.host || h.fqdn == msg.host
        end

        next if host.nil?

        # Update host's collectors
        @hosts_collectors[host.name].each { |collector| collector << msg }
      end
    end

    def run_flare_settler
      loop do
        @hosts_collectors.each_value do |collectors|
          collectors.each { |c| c.settle_flares }
        end

        sleep(5)
      end
    end

    def open_pipe(config)
      f = File.open(config.syslog_pipe)
      # F_SETPIPE_SZ
      f.fcntl(1031, config.pipe_size)
      return f unless config.mbuffer.enable

      f.close
      r, w = IO.pipe
      pid = Process.spawn(
        config.mbuffer.path,
        '-q',
        '-m', config.mbuffer.size,
        '-i', config.syslog_pipe,
        out: w,
      )

      w.close
      r
    end
  end
end
