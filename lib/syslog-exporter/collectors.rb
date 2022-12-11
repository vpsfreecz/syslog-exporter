module SyslogExporter
  module Collectors
    def self.register(name, klass)
      @collectors ||= {}
      @collectors[name] = klass
    end

    def self.each(&block)
      (@collectors || {}).each(&block)
    end
  end
end
