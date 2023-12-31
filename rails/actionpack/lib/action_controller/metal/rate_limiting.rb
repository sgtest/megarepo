# frozen_string_literal: true

module ActionController # :nodoc:
  module RateLimiting
    extend ActiveSupport::Concern

    module ClassMethods
      # Applies a rate limit to all actions or those specified by the normal <tt>before_action</tt> filters with <tt>only:</tt> and <tt>except:</tt>.
      #
      # The maximum number of requests allowed is specified <tt>to:</tt> and constrained to the window of time given by <tt>within:</tt>.
      #
      # Rate limits are by default unique to the ip address making the request, but you can provide your own identity function by passing a callable
      # in the <tt>by:</tt> parameter. It's evaluated within the context of the controller processing the request.
      #
      # Requests that exceed the rate limit are refused with a <tt>429 Too Many Requests</tt> response. You can specialize this by passing a callable
      # in the <tt>with:</tt> parameter. It's evaluated within the context of the controller processing the request.
      #
      # Examples:
      #
      #   class SessionsController < ApplicationController
      #     rate_limit to: 10, within: 3.minutes, only: :create
      #   end
      #
      #   class SignupsController < ApplicationController
      #     rate_limit to: 1000, within: 10.seconds,
      #       by: -> { request.domain }, with: -> { redirect_to busy_controller_url, alert: "Too many signups on domain!" }, only: :new
      #   end
      #
      # Note: Rate limiting relies on the application having an accessible Redis server and on Kredis 1.7.0+ being available in the bundle.
      # This uses the Kredis limiter type underneath, which is failsafe, so in case Redis is inaccessible, the rate limit will not refuse action execution.
      def rate_limit(to:, within:, by: -> { request.remote_ip }, with: -> { head :too_many_requests }, **options)
        require_compatible_kredis
        before_action -> { rate_limiting(to: to, within: within, by: by, with: with) }, **options
      end

      private
        def require_compatible_kredis
          require "kredis"

          if Kredis::VERSION < "1.7.0"
            raise StandardError, \
              "Rate limiting requires Kredis 1.7.0+. Please update by calling `bundle update kredis`."
          end
        rescue LoadError
          raise LoadError, \
            "Rate limiting requires Redis and Kredis. " +
            "Please ensure you have Redis installed on your system and the Kredis gem in your Gemfile."
        end
    end

    private
      def rate_limiting(to:, within:, by:, with:)
        limiter = Kredis.limiter "rate-limit:#{controller_path}:#{instance_exec(&by)}", limit: to, expires_in: within

        if limiter.exceeded?
          ActiveSupport::Notifications.instrument("rate_limit.action_controller", request: request) do
            instance_exec(&with)
          end
        else
          limiter.poke
        end
      end
  end
end
