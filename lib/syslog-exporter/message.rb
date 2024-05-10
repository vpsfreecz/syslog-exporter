module SyslogExporter
  # Represents one syslog message
  class Message
    # @return [Time]
    attr_reader :time

    # @return [String]
    attr_reader :host

    # @return [String, nil]
    attr_reader :program

    # @return [Integer, nil]
    attr_reader :pid

    # @return [String]
    attr_reader :message

    def initialize(time:, host:, message:, program: nil, pid: nil)
      @time = time
      @host = host
      @program = program
      @pid = pid
      @message = message
    end

    def to_s
      ret = []
      ret << "time=#{time.iso8601}"
      ret << "host=#{host}"
      ret << "program=#{program}" if program
      ret << "pid=#{pid}" if pid
      ret << "message=#{message.inspect}"
      ret.join(' ')
    end
  end
end
