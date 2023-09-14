# frozen_string_literal: true

require "cases/helper"
require "models/topic"

class TransactionInstrumentationTest < ActiveRecord::TestCase
  self.use_transactional_tests = false
  fixtures :topics

  def test_transaction_instrumentation_on_commit
    topic = topics(:fifth)

    notified = false
    subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
      assert event.payload[:connection]
      assert_equal :commit, event.payload[:outcome]
      notified = true
    end

    ActiveRecord::Base.transaction do
      topic.update(title: "Ruby on Rails")
    end

    assert notified
  ensure
    ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
  end

  def test_transaction_instrumentation_on_rollback
    topic = topics(:fifth)

    notified = false
    subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
      assert event.payload[:connection]
      assert_equal :rollback, event.payload[:outcome]
      notified = true
    end

    ActiveRecord::Base.transaction do
      topic.update(title: "Ruby on Rails")
      raise ActiveRecord::Rollback
    end

    assert notified
  ensure
    ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
  end

  def test_transaction_instrumentation_with_savepoints
    topic = topics(:fifth)

    events = []
    subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
      events << event
    end

    ActiveRecord::Base.transaction do
      topic.update(title: "Sinatra")
      ActiveRecord::Base.transaction(requires_new: true) do
        topic.update(title: "Ruby on Rails")
      end
    end

    assert_equal 2, events.count
    savepoint, real = events
    assert_equal :commit, savepoint.payload[:outcome]
    assert_equal :commit, real.payload[:outcome]
  ensure
    ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
  end

  def test_transaction_instrumentation_with_restart_parent_transaction_on_commit
    topic = topics(:fifth)

    events = []
    subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
      events << event
    end

    ActiveRecord::Base.transaction do
      ActiveRecord::Base.transaction(requires_new: true) do
        topic.update(title: "Ruby on Rails")
      end
    end

    assert_equal 1, events.count
  ensure
    ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
  end

  def test_transaction_instrumentation_with_restart_parent_transaction_on_rollback
    topic = topics(:fifth)

    events = []
    subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
      events << event
    end

    ActiveRecord::Base.transaction do
      ActiveRecord::Base.transaction(requires_new: true) do
        topic.update(title: "Ruby on Rails")
        raise ActiveRecord::Rollback
      end
      raise ActiveRecord::Rollback
    end

    assert_equal 2, events.count
    restart, real = events
    assert_equal :restart, restart.payload[:outcome]
    assert_equal :rollback, real.payload[:outcome]
  ensure
    ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
  end

  def test_transaction_instrumentation_with_unmaterialized_restart_parent_transactions
    events = []
    subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
      events << event
    end

    ActiveRecord::Base.transaction do
      ActiveRecord::Base.transaction(requires_new: true) do
        raise ActiveRecord::Rollback
      end
    end

    assert_equal 0, events.count
  ensure
    ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
  end

  def test_transaction_instrumentation_with_restart_savepoint_parent_transactions
    topic = topics(:fifth)

    events = []
    subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
      events << event
    end

    ActiveRecord::Base.transaction do
      topic.update(title: "Sinatry")
      ActiveRecord::Base.transaction(requires_new: true) do
        ActiveRecord::Base.transaction(requires_new: true) do
          topic.update(title: "Ruby on Rails")
          raise ActiveRecord::Rollback
        end
      end
    end

    assert_equal 3, events.count
    restart, savepoint, real = events
    assert_equal :restart, restart.payload[:outcome]
    assert_equal :commit, savepoint.payload[:outcome]
    assert_equal :commit, real.payload[:outcome]
  ensure
    ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
  end

  def test_transaction_instrumentation_only_fires_if_materialized
    notified = false
    subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
      notified = true
    end

    ActiveRecord::Base.transaction do
    end

    assert_not notified
  ensure
    ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
  end

  def test_transaction_instrumentation_fires_before_after_commit_callbacks
    notified = false
    after_commit_triggered = false

    topic_model = Class.new(ActiveRecord::Base) do
      self.table_name = "topics"

      after_commit do
        after_commit_triggered = true
      end
    end

    subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
      assert_not after_commit_triggered, "Transaction notification fired after the after_commit callback"
      notified = true
    end

    topic_model.create!

    assert notified
    assert after_commit_triggered
  ensure
    ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
  end

  def test_transaction_instrumentation_fires_before_after_rollback_callbacks
    notified = false
    after_rollback_triggered = false

    topic_model = Class.new(ActiveRecord::Base) do
      self.table_name = "topics"

      after_rollback do
        after_rollback_triggered = true
      end
    end

    subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
      assert_not after_rollback_triggered, "Transaction notification fired after the after_rollback callback"
      notified = true
    end

    topic_model.transaction do
      topic_model.create!
      raise ActiveRecord::Rollback
    end

    assert notified
    assert after_rollback_triggered
  ensure
    ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
  end

  def test_transaction_instrumentation_on_failed_commit
    topic = topics(:fifth)

    notified = false
    subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
      notified = true
    end

    error = Class.new(StandardError)
    assert_raises error do
      ActiveRecord::Base.connection.stub(:commit_db_transaction, -> (*) { raise error }) do
        ActiveRecord::Base.transaction do
          topic.update(title: "Ruby on Rails")
        end
      end
    end

    assert notified
  ensure
    ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
  end

  unless in_memory_db?
    def test_transaction_instrumentation_on_failed_rollback
      topic = topics(:fifth)

      notified = false
      subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
        assert_equal :incomplete, event.payload[:outcome]
        notified = true
      end

      error = Class.new(StandardError)
      assert_raises error do
        ActiveRecord::Base.connection.stub(:rollback_db_transaction, -> (*) { raise error }) do
          ActiveRecord::Base.transaction do
            topic.update(title: "Ruby on Rails")
            raise ActiveRecord::Rollback
          end
        end
      end

      assert notified
    ensure
      ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
    end
  end

  def test_transaction_instrumentation_on_broken_subscription
    topic = topics(:fifth)

    error = Class.new(StandardError)
    subscriber = ActiveSupport::Notifications.subscribe("transaction.active_record") do |event|
      raise error
    end

    assert_raises(error) do
      ActiveRecord::Base.transaction do
        topic.update(title: "Ruby on Rails")
      end
    end
  ensure
    ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
  end
end
