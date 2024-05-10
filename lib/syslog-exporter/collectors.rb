module SyslogExporter
  module Collectors
    def self.register(name, klass)
      @collectors ||= {}
      @collectors[name] = klass
    end

    def self.each(&)
      (@collectors || {}).each(&)
    end

    def self.each_value(&)
      (@collectors || {}).each_value(&)
    end
  end
end
