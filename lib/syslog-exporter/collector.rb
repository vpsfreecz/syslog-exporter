require 'thread'

module SyslogExporter
  # Base class for collectors
  class Collector
    class << self
      # Register collector class
      # @param name [Symbol]
      def register(name)
        Collectors.register(name, self)
      end

      # Override to register mettrics to the registry using {#add_metric}
      def setup(registry)

      end

      # Add metric to the registry
      def add_metric(registry, metric_type, name, docstring:, labels: [])
        @metrics ||= {}
        @metrics[name] = registry.send(
          metric_type,
          name,
          docstring: docstring,
          labels: (labels + %i(alias fqdn)).uniq,
        )
      end

      # Return a hash of registered metrics
      def metrics
        @metrics
      end

      # @param host [Config::Host]
      def use_host?(host)
        true
      end
    end

    # @return [Config::Host]
    attr_reader :host

    # @param host [Config::Host]
    def initialize(host)
      @host = host
      @flares = {}
      @flare_mutex = Mutex.new
      setup
    end

    def setup

    end

    # Process syslog message
    # @param message [Message]
    def <<(message)

    end

    # Remove expired flares
    def settle_flares
      to_set = []

      @flare_mutex.synchronize do
        next if @flares.empty?

        now = Time.now

        @flares.delete_if do |name, flare|
          if now > flare.ends_at
            to_set << flare
            true
          else
            false
          end
        end
      end

      to_set.each do |flare|
        set_gauge(flare.name, flare.reset_value, labels: flare.labels)
      end
    end

    protected
    def set_gauge(name, value, labels: {})
      metrics[name].set(value, labels: labels.merge(metric_labels))
    end

    def increment_counter(name, labels: {})
      metrics[name].increment(labels: labels.merge(metric_labels))
    end

    def set_flare(name, value, labels: {}, seconds: 120)
      set_gauge(name, value, labels: labels)

      @flare_mutex.synchronize do
        @flares[name] = Flare.new(
          name,
          labels: labels,
          seconds: seconds,
        )
      end
    end

    def unset_flare(name, labels: {})
      flare = @flare_mutex.synchronize { @flares.delete(name) }
      return if flare.nil?

      set_gauge(flare.name, flare.reset_value, labels: flare.labels)
    end

    def metric_labels
      {
        alias: host.alias_name,
        fqdn: host.fqdn,
      }
    end

    def metrics
      self.class.metrics
    end
  end
end
