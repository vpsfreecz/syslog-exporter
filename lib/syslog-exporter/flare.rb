module SyslogExporter
  class Flare
    # @return [Symbol]
    attr_reader :name

    # @return [Hash]
    attr_reader :labels

    # @return [Time]
    attr_reader :ends_at

    # @param name [Symbol]
    # @param labels [Hash]
    # @param seconds [Integer]
    def initialize(name, labels: {}, seconds: 120)
      @name = name
      @labels = labels
      @ends_at = Time.now + seconds
    end

    # @return [Integer]
    def reset_value
      0
    end
  end
end
