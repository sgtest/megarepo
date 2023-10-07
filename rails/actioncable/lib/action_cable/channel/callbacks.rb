# frozen_string_literal: true

require "active_support/callbacks"

module ActionCable
  module Channel
    # = Action Cable \Channel \Callbacks
    #
    # Action Cable Channel provides callback hooks that are invoked during the
    # life cycle of a channel:
    #
    # * {before_subscribe}[rdoc-ref:ClassMethods#before_subscribe]
    # * {after_subscribe}[rdoc-ref:ClassMethods#after_subscribe] (aliased as
    #   {on_subscribe}[rdoc-ref:ClassMethods#on_subscribe])
    # * {before_unsubscribe}[rdoc-ref:ClassMethods#before_unsubscribe]
    # * {after_unsubscribe}[rdoc-ref:ClassMethods#after_unsubscribe] (aliased as
    #   {on_unsubscribe}[rdoc-ref:ClassMethods#on_unsubscribe])
    #
    # NOTE: the <tt>after_subscribe</tt> callback is triggered whenever
    # the <tt>subscribed</tt> method is called, even if subscription was rejected
    # with the <tt>reject</tt> method.
    # To trigger <tt>after_subscribe</tt> only on successful subscriptions,
    # use <tt>after_subscribe :my_method_name, unless: :subscription_rejected?</tt>
    #
    module Callbacks
      extend  ActiveSupport::Concern
      include ActiveSupport::Callbacks

      included do
        define_callbacks :subscribe
        define_callbacks :unsubscribe
      end

      module ClassMethods
        def before_subscribe(*methods, &block)
          set_callback(:subscribe, :before, *methods, &block)
        end

        def after_subscribe(*methods, &block)
          set_callback(:subscribe, :after, *methods, &block)
        end
        alias_method :on_subscribe, :after_subscribe

        def before_unsubscribe(*methods, &block)
          set_callback(:unsubscribe, :before, *methods, &block)
        end

        def after_unsubscribe(*methods, &block)
          set_callback(:unsubscribe, :after, *methods, &block)
        end
        alias_method :on_unsubscribe, :after_unsubscribe
      end
    end
  end
end
