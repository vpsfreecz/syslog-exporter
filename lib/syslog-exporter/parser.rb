require 'time'

module SyslogExporter
  class Parser
    # @param io [IO]
    def initialize(io)
      @io = io
    end

    # @yieldparam [Message, :eof]
    def each_message
      it = @io.each_line
      it.next # drop the first line in case it is incomplete

      it.each do |line|
        msg = parse_message(line)

        if msg.nil?
          warn "Unknown message #{line.inspect}"
          next
        end

        yield(msg)
      end

      yield(:eof)
    end

    protected
    def parse_message(line)
      time_str, host_line = next_value(line)
      return if time_str.nil?

      time = Time.iso8601(time_str)

      host_str, prog_line = next_value(host_line)
      return if host_str.nil?

      host = nil
      program = nil
      pid = nil
      message = nil

      if host_str == 'localhost' && /^\d+\-\d+\-\d+_\d+ \d+\:\d+\.\d+ / =~ prog_line
        # Parse messages forwarded to syslog from svlogd -tt. With -tt, each log
        # message starts with date and time. We then rely on svlogd beign configured
        # to prefix each line with host fqdn and program name.
        svlogd_date, svlogd_time_line = next_value(prog_line)
        return if svlogd_date.nil?

        svlogd_time, svlogd_host_line = next_value(svlogd_time_line)
        return if svlogd_time.nil?

        svlogd_host_str, svlogd_prog_line = next_value(svlogd_host_line)
        return if svlogd_host_str.nil?

        svlogd_prog_str, msg_line = next_value(svlogd_prog_line)
        return if svlogd_prog_str.nil?

        host = svlogd_host_str
        program = svlogd_prog_str
        message = msg_line
      else
        host = host_str

        prog_str, msg_line = next_value(prog_line)

        if prog_str
          if prog_str == 'kernel' || prog_str == 'kernel[]'
            program = 'kernel'
          elsif /([^\[]+)\[(\d+)\]$/ =~ prog_str
            program = $1
            pid = $2.to_i
          else
            program = prog_str
          end
        end

        message = msg_line
      end

      Message.new(
        time: time,
        host: host,
        program: program,
        pid: pid,
        message: message,
      )
    end

    def next_value(str)
      space = str.index(' ')
      return [nil, str] if space.nil?

      [str[0..(space-1)], str[(space+1)..-1]]
    end
  end
end
