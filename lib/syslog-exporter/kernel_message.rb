require_relative 'message'

module SyslogExporter
  class KernelMessage < Message
    # @return [String, nil] syslog namespace tag
    attr_reader :syslogns_tag

    def initialize(*, **)
      super
      @syslogns_tag = extract_syslogns
    end

    protected

    def extract_syslogns
      return if /^\[\s*\d+\.\d+\] \[ \s*([a-zA-Z0-9_:-]+)\s* \] [^$]+/ !~ message

      ::Regexp.last_match(1)
    end
  end
end
