# frozen_string_literal: true

require "cases/helper"
require "support/connection_helper"

class ConnectionTest < ActiveRecord::AbstractMysqlTestCase
  include ConnectionHelper

  def setup
    super
    @subscriber = SQLSubscriber.new
    @subscription = ActiveSupport::Notifications.subscribe("sql.active_record", @subscriber)
    @connection = ActiveRecord::Base.connection
  end

  def teardown
    ActiveSupport::Notifications.unsubscribe(@subscription)
    super
  end

  def test_bad_connection
    assert_raise ActiveRecord::NoDatabaseError do
      db_config = ActiveRecord::Base.configurations.configs_for(env_name: "arunit", name: "primary")
      configuration = db_config.configuration_hash.merge(database: "inexistent_activerecord_unittest")
      connection = if current_adapter?(:Mysql2Adapter)
        ActiveRecord::Base.mysql2_connection(configuration)
      else
        ActiveRecord::Base.trilogy_connection(configuration)
      end
      connection.drop_table "ex", if_exists: true
    end
  end

  def test_no_automatic_reconnection_after_timeout
    assert_predicate @connection, :active?
    cause_server_side_disconnect
    assert_not_predicate @connection, :active?
  ensure
    # Repair all fixture connections so other tests won't break.
    @fixture_connections.each(&:verify!)
  end

  def test_successful_reconnection_after_timeout_with_manual_reconnect
    assert_predicate @connection, :active?
    cause_server_side_disconnect
    @connection.reconnect!
    assert_predicate @connection, :active?
  end

  def test_successful_reconnection_after_timeout_with_verify
    assert_predicate @connection, :active?
    cause_server_side_disconnect
    @connection.verify!
    assert_predicate @connection, :active?
  end

  def test_execute_after_disconnect_reconnects
    @connection.disconnect!

    assert_equal 3, @connection.select_value("SELECT 1+2")
  end

  def test_quote_after_disconnect_reconnects
    @connection.disconnect!

    assert_equal "'string'", @connection.quote("string")
  end

  def test_active_after_disconnect
    @connection.disconnect!
    assert_equal false, @connection.active?
  end

  def test_wait_timeout_as_string
    run_without_connection do |orig_connection|
      ActiveRecord::Base.establish_connection(orig_connection.merge(wait_timeout: "60"))
      result = ActiveRecord::Base.connection.select_value("SELECT @@SESSION.wait_timeout")
      assert_equal 60, result
    end
  end

  def test_wait_timeout_as_url
    run_without_connection do |orig_connection|
      ActiveRecord::Base.establish_connection(orig_connection.merge("url" => "#{orig_connection[:adapter]}:///?wait_timeout=60"))
      result = ActiveRecord::Base.connection.select_value("SELECT @@SESSION.wait_timeout")
      assert_equal 60, result
    end
  end

  def test_character_set_connection_is_configured
    run_without_connection do |orig_connection|
      configuration_hash = orig_connection.except(:encoding, :collation)
      ActiveRecord::Base.establish_connection(configuration_hash.merge!(encoding: "cp932"))
      connection = ActiveRecord::Base.connection

      assert_equal "cp932", connection.show_variable("character_set_client")
      assert_equal "cp932", connection.show_variable("character_set_results")
      assert_equal "cp932", connection.show_variable("character_set_connection")
      assert_equal "cp932_japanese_ci", connection.show_variable("collation_connection")

      expected = "こんにちは".encode(Encoding::CP932)
      assert_equal expected, connection.query_value("SELECT 'こんにちは'")
    end
  end

  def test_collation_connection_is_configured
    assert_equal "utf8mb4_unicode_ci", @connection.show_variable("collation_connection")
    assert_equal 1, @connection.query_value("SELECT 'こんにちは' = 'コンニチハ'")

    assert_equal "utf8mb4_general_ci", ARUnit2Model.connection.show_variable("collation_connection")
    assert_equal 0, ARUnit2Model.connection.query_value("SELECT 'こんにちは' = 'コンニチハ'")
  end

  def test_mysql_default_in_strict_mode
    result = @connection.select_value("SELECT @@SESSION.sql_mode")
    assert_match %r(STRICT_ALL_TABLES), result
  end

  def test_mysql_strict_mode_disabled
    run_without_connection do |orig_connection|
      ActiveRecord::Base.establish_connection(orig_connection.merge(strict: false))
      result = ActiveRecord::Base.connection.select_value("SELECT @@SESSION.sql_mode")
      assert_no_match %r(STRICT_ALL_TABLES), result
    end
  end

  def test_mysql_strict_mode_specified_default
    run_without_connection do |orig_connection|
      ActiveRecord::Base.establish_connection(orig_connection.merge(strict: :default))
      global_sql_mode = ActiveRecord::Base.connection.select_value("SELECT @@GLOBAL.sql_mode")
      session_sql_mode = ActiveRecord::Base.connection.select_value("SELECT @@SESSION.sql_mode")
      assert_equal global_sql_mode, session_sql_mode
    end
  end

  def test_mysql_sql_mode_variable_overrides_strict_mode
    run_without_connection do |orig_connection|
      ActiveRecord::Base.establish_connection(orig_connection.deep_merge(variables: { "sql_mode" => "ansi" }))
      result = ActiveRecord::Base.connection.select_value("SELECT @@SESSION.sql_mode")
      assert_no_match %r(STRICT_ALL_TABLES), result
    end
  end

  unless current_adapter?(:TrilogyAdapter)
    def test_passing_arbitrary_flags_to_adapter
      run_without_connection do |orig_connection|
        ActiveRecord::Base.establish_connection(orig_connection.merge(flags: Mysql2::Client::COMPRESS))
        assert_equal (Mysql2::Client::COMPRESS | Mysql2::Client::FOUND_ROWS), ActiveRecord::Base.connection.raw_connection.query_options[:flags]
      end
    end

    def test_passing_flags_by_array_to_adapter
      run_without_connection do |orig_connection|
        ActiveRecord::Base.establish_connection(orig_connection.merge(flags: ["COMPRESS"]))
        assert_equal ["COMPRESS", "FOUND_ROWS"], ActiveRecord::Base.connection.raw_connection.query_options[:flags]
      end
    end
  end

  def test_mysql_set_session_variable
    run_without_connection do |orig_connection|
      ActiveRecord::Base.establish_connection(orig_connection.deep_merge(variables: { default_week_format: 3 }))
      session_mode = ActiveRecord::Base.connection.exec_query "SELECT @@SESSION.DEFAULT_WEEK_FORMAT"
      assert_equal 3, session_mode.rows.first.first.to_i
    end
  end

  def test_mysql_set_session_variable_to_default
    run_without_connection do |orig_connection|
      ActiveRecord::Base.establish_connection(orig_connection.deep_merge(variables: { default_week_format: :default }))
      global_mode = ActiveRecord::Base.connection.exec_query "SELECT @@GLOBAL.DEFAULT_WEEK_FORMAT"
      session_mode = ActiveRecord::Base.connection.exec_query "SELECT @@SESSION.DEFAULT_WEEK_FORMAT"
      assert_equal global_mode.rows, session_mode.rows
    end
  end

  def test_logs_name_show_variable
    ActiveRecord::Base.connection.materialize_transactions
    @subscriber.logged.clear
    @connection.show_variable "foo"
    assert_equal "SCHEMA", @subscriber.logged[0][1]
  end

  def test_logs_name_rename_column_for_alter
    @connection.execute "CREATE TABLE `bar_baz` (`foo` varchar(255))"
    @subscriber.logged.clear
    @connection.send(:rename_column_for_alter, "bar_baz", "foo", "foo2")
    if @connection.send(:supports_rename_column?)
      assert_empty @subscriber.logged
    else
      assert_equal "SCHEMA", @subscriber.logged[0][1]
    end
  ensure
    @connection.execute "DROP TABLE `bar_baz`"
  end

  def test_get_and_release_advisory_lock
    lock_name = "test lock'n'name"

    got_lock = @connection.get_advisory_lock(lock_name)
    assert got_lock, "get_advisory_lock should have returned true but it didn't"

    assert_equal false, test_lock_free(lock_name),
      "expected the test advisory lock to be held but it wasn't"

    released_lock = @connection.release_advisory_lock(lock_name)
    assert released_lock, "expected release_advisory_lock to return true but it didn't"

    assert test_lock_free(lock_name), "expected the test lock to be available after releasing"
  end

  def test_release_non_existent_advisory_lock
    lock_name = "fake lock'n'name"
    released_non_existent_lock = @connection.release_advisory_lock(lock_name)
    assert_equal false, released_non_existent_lock,
      "expected release_advisory_lock to return false when there was no lock to release"
  end

  private
    def cause_server_side_disconnect
      @connection.update("set @@wait_timeout=1")
      sleep 2
    end

    def test_lock_free(lock_name)
      @connection.select_value("SELECT IS_FREE_LOCK(#{@connection.quote(lock_name)})") == 1
    end
end
