# frozen_string_literal: true

require "cases/helper"
require "support/connection_helper"
require "models/book"
require "models/post"
require "models/author"
require "models/event"

module ActiveRecord
  class AdapterTest < ActiveRecord::TestCase
    def setup
      @connection = ActiveRecord::Base.connection
      @connection.materialize_transactions
    end

    ##
    # PostgreSQL does not support null bytes in strings
    unless current_adapter?(:PostgreSQLAdapter) ||
        (current_adapter?(:SQLite3Adapter) && !ActiveRecord::Base.connection.prepared_statements)
      def test_update_prepared_statement
        b = Book.create(name: "my \x00 book")
        b.reload
        assert_equal "my \x00 book", b.name
        b.update(name: "my other \x00 book")
        b.reload
        assert_equal "my other \x00 book", b.name
      end
    end

    def test_create_record_with_pk_as_zero
      Book.create(id: 0)
      assert_equal 0, Book.find(0).id
      assert_nothing_raised { Book.destroy(0) }
    end

    def test_valid_column
      @connection.native_database_types.each_key do |type|
        assert @connection.valid_type?(type)
      end
    end

    def test_invalid_column
      assert_not @connection.valid_type?(:foobar)
    end

    def test_tables
      tables = @connection.tables
      assert_includes tables, "accounts"
      assert_includes tables, "authors"
      assert_includes tables, "tasks"
      assert_includes tables, "topics"
    end

    def test_table_exists?
      assert @connection.table_exists?("accounts")
      assert @connection.table_exists?(:accounts)
      assert_not @connection.table_exists?("nonexistingtable")
      assert_not @connection.table_exists?("'")
      assert_not @connection.table_exists?(nil)
    end

    def test_data_sources
      data_sources = @connection.data_sources
      assert_includes data_sources, "accounts"
      assert_includes data_sources, "authors"
      assert_includes data_sources, "tasks"
      assert_includes data_sources, "topics"
    end

    def test_data_source_exists?
      assert @connection.data_source_exists?("accounts")
      assert @connection.data_source_exists?(:accounts)
      assert_not @connection.data_source_exists?("nonexistingtable")
      assert_not @connection.data_source_exists?("'")
      assert_not @connection.data_source_exists?(nil)
    end

    def test_indexes
      idx_name = "accounts_idx"

      indexes = @connection.indexes("accounts")
      assert_empty indexes

      @connection.add_index :accounts, :firm_id, name: idx_name
      indexes = @connection.indexes("accounts")
      assert_equal "accounts", indexes.first.table
      assert_equal idx_name, indexes.first.name
      assert_not indexes.first.unique
      assert_equal ["firm_id"], indexes.first.columns
    ensure
      @connection.remove_index(:accounts, name: idx_name) rescue nil
    end

    def test_remove_index_when_name_and_wrong_column_name_specified
      index_name = "accounts_idx"

      @connection.add_index :accounts, :firm_id, name: index_name
      assert_raises ArgumentError do
        @connection.remove_index :accounts, name: index_name, column: :wrong_column_name
      end
    ensure
      @connection.remove_index(:accounts, name: index_name)
    end

    def test_remove_index_when_name_and_wrong_column_name_specified_positional_argument
      index_name = "accounts_idx"

      @connection.add_index :accounts, :firm_id, name: index_name
      assert_raises ArgumentError do
        @connection.remove_index :accounts, :wrong_column_name, name: index_name
      end
    ensure
      @connection.remove_index(:accounts, name: index_name)
    end

    def test_current_database
      if @connection.respond_to?(:current_database)
        assert_equal ARTest.test_configuration_hashes["arunit"]["database"], @connection.current_database
      end
    end

    def test_exec_query_returns_an_empty_result
      result = @connection.exec_query "INSERT INTO subscribers(nick) VALUES('me')"
      assert_instance_of(ActiveRecord::Result, result)
    end

    if current_adapter?(:Mysql2Adapter, :TrilogyAdapter)
      def test_charset
        assert_not_nil @connection.charset
        assert_not_equal "character_set_database", @connection.charset
        assert_equal @connection.show_variable("character_set_database"), @connection.charset
      end

      def test_collation
        assert_not_nil @connection.collation
        assert_not_equal "collation_database", @connection.collation
        assert_equal @connection.show_variable("collation_database"), @connection.collation
      end

      def test_show_nonexistent_variable_returns_nil
        assert_nil @connection.show_variable("foo_bar_baz")
      end

      def test_not_specifying_database_name_for_cross_database_selects
        assert_nothing_raised do
          db_config = ActiveRecord::Base.configurations.configs_for(env_name: "arunit", name: "primary")
          ActiveRecord::Base.establish_connection(db_config.configuration_hash.except(:database))

          config = ARTest.test_configuration_hashes
          ActiveRecord::Base.connection.execute(
            "SELECT #{config['arunit']['database']}.pirates.*, #{config['arunit2']['database']}.courses.* " \
            "FROM #{config['arunit']['database']}.pirates, #{config['arunit2']['database']}.courses"
          )
        end
      ensure
        ActiveRecord::Base.establish_connection :arunit
      end
    end

    unless in_memory_db?
      def test_disable_prepared_statements
        original_prepared_statements = ActiveRecord.disable_prepared_statements
        db_config = ActiveRecord::Base.configurations.configs_for(env_name: "arunit", name: "primary")
        ActiveRecord::Base.establish_connection(db_config.configuration_hash.merge(prepared_statements: true))

        assert_predicate ActiveRecord::Base.connection, :prepared_statements?

        ActiveRecord.disable_prepared_statements = true
        ActiveRecord::Base.establish_connection(db_config.configuration_hash.merge(prepared_statements: true))
        assert_not_predicate ActiveRecord::Base.connection, :prepared_statements?
      ensure
        ActiveRecord.disable_prepared_statements = original_prepared_statements
        ActiveRecord::Base.establish_connection :arunit
      end
    end

    def test_table_alias
      def @connection.test_table_alias_length() 10; end
      class << @connection
        alias_method :old_table_alias_length, :table_alias_length
        alias_method :table_alias_length,     :test_table_alias_length
      end

      assert_equal "posts",      @connection.table_alias_for("posts")
      assert_equal "posts_comm", @connection.table_alias_for("posts_comments")
      assert_equal "dbo_posts",  @connection.table_alias_for("dbo.posts")

      class << @connection
        remove_method :table_alias_length
        alias_method :table_alias_length, :old_table_alias_length
      end
    end

    def test_uniqueness_violations_are_translated_to_specific_exception
      @connection.execute "INSERT INTO subscribers(nick) VALUES('me')"
      error = assert_raises(ActiveRecord::RecordNotUnique) do
        @connection.execute "INSERT INTO subscribers(nick) VALUES('me')"
      end

      assert_not_nil error.cause
    end

    def test_not_null_violations_are_translated_to_specific_exception
      error = assert_raises(ActiveRecord::NotNullViolation) do
        Post.create
      end

      assert_not_nil error.cause
    end

    unless current_adapter?(:SQLite3Adapter)
      def test_value_limit_violations_are_translated_to_specific_exception
        error = assert_raises(ActiveRecord::ValueTooLong) do
          Event.create(title: "abcdefgh")
        end

        assert_not_nil error.cause
      end

      def test_numeric_value_out_of_ranges_are_translated_to_specific_exception
        error = assert_raises(ActiveRecord::RangeError) do
          Book.connection.create("INSERT INTO books(author_id) VALUES (9223372036854775808)")
        end

        assert_not_nil error.cause
      end
    end

    def test_exceptions_from_notifications_are_not_translated
      original_error = StandardError.new("This StandardError shouldn't get translated")
      subscriber = ActiveSupport::Notifications.subscribe("sql.active_record") { raise original_error }
      actual_error = assert_raises(StandardError) do
        @connection.execute("SELECT * FROM posts")
      end
      assert_equal original_error, actual_error

    ensure
      ActiveSupport::Notifications.unsubscribe(subscriber) if subscriber
    end

    def test_database_related_exceptions_are_translated_to_statement_invalid
      error = assert_raises(ActiveRecord::StatementInvalid) do
        @connection.execute("This is a syntax error")
      end

      assert_instance_of ActiveRecord::StatementInvalid, error
      assert_kind_of Exception, error.cause
    end

    def test_select_all_always_return_activerecord_result
      result = @connection.select_all "SELECT * FROM posts"
      assert result.is_a?(ActiveRecord::Result)
    end

    if ActiveRecord::Base.connection.prepared_statements
      def test_select_all_insert_update_delete_with_casted_binds
        binds = [Event.type_for_attribute("id").serialize(1)]
        bind_param = Arel::Nodes::BindParam.new(nil)

        id = @connection.insert("INSERT INTO events(id) VALUES (#{bind_param.to_sql})", nil, nil, nil, nil, binds)
        assert_equal 1, id

        updated = @connection.update("UPDATE events SET title = 'foo' WHERE id = #{bind_param.to_sql}", nil, binds)
        assert_equal 1, updated

        result = @connection.select_all("SELECT * FROM events WHERE id = #{bind_param.to_sql}", nil, binds)
        assert_equal({ "id" => 1, "title" => "foo" }, result.first)

        deleted = @connection.delete("DELETE FROM events WHERE id = #{bind_param.to_sql}", nil, binds)
        assert_equal 1, deleted

        result = @connection.select_all("SELECT * FROM events WHERE id = #{bind_param.to_sql}", nil, binds)
        assert_nil result.first
      end

      def test_select_all_insert_update_delete_with_binds
        binds = [Relation::QueryAttribute.new("id", 1, Event.type_for_attribute("id"))]
        bind_param = Arel::Nodes::BindParam.new(nil)

        id = @connection.insert("INSERT INTO events(id) VALUES (#{bind_param.to_sql})", nil, nil, nil, nil, binds)
        assert_equal 1, id

        updated = @connection.update("UPDATE events SET title = 'foo' WHERE id = #{bind_param.to_sql}", nil, binds)
        assert_equal 1, updated

        result = @connection.select_all("SELECT * FROM events WHERE id = #{bind_param.to_sql}", nil, binds)
        assert_equal({ "id" => 1, "title" => "foo" }, result.first)

        deleted = @connection.delete("DELETE FROM events WHERE id = #{bind_param.to_sql}", nil, binds)
        assert_equal 1, deleted

        result = @connection.select_all("SELECT * FROM events WHERE id = #{bind_param.to_sql}", nil, binds)
        assert_nil result.first
      end
    end

    def test_select_methods_passing_a_association_relation
      author = Author.create!(name: "john")
      Post.create!(author: author, title: "foo", body: "bar")
      query = author.posts.where(title: "foo").select(:title)
      assert_equal({ "title" => "foo" }, @connection.select_one(query))
      assert @connection.select_all(query).is_a?(ActiveRecord::Result)
      assert_equal "foo", @connection.select_value(query)
      assert_equal ["foo"], @connection.select_values(query)
    end

    def test_select_methods_passing_a_relation
      Post.create!(title: "foo", body: "bar")
      query = Post.where(title: "foo").select(:title)
      assert_equal({ "title" => "foo" }, @connection.select_one(query))
      assert @connection.select_all(query).is_a?(ActiveRecord::Result)
      assert_equal "foo", @connection.select_value(query)
      assert_equal ["foo"], @connection.select_values(query)
    end

    test "type_to_sql returns a String for unmapped types" do
      assert_equal "special_db_type", @connection.type_to_sql(:special_db_type)
    end
  end

  class AdapterForeignKeyTest < ActiveRecord::TestCase
    self.use_transactional_tests = false

    fixtures :fk_test_has_pk

    def setup
      @connection = ActiveRecord::Base.connection
    end

    def test_foreign_key_violations_are_translated_to_specific_exception_with_validate_false
      klass_has_fk = Class.new(ActiveRecord::Base) do
        self.table_name = "fk_test_has_fk"
      end

      error = assert_raises(ActiveRecord::InvalidForeignKey) do
        has_fk = klass_has_fk.new
        has_fk.fk_id = 1231231231
        has_fk.save(validate: false)
      end

      assert_not_nil error.cause
    end

    def test_foreign_key_violations_on_insert_are_translated_to_specific_exception
      error = assert_raises(ActiveRecord::InvalidForeignKey) do
        insert_into_fk_test_has_fk
      end

      assert_not_nil error.cause
    end

    def test_foreign_key_violations_on_delete_are_translated_to_specific_exception
      insert_into_fk_test_has_fk fk_id: 1

      error = assert_raises(ActiveRecord::InvalidForeignKey) do
        @connection.execute "DELETE FROM fk_test_has_pk WHERE pk_id = 1"
      end

      assert_not_nil error.cause
    end

    def test_disable_referential_integrity
      assert_nothing_raised do
        @connection.disable_referential_integrity do
          insert_into_fk_test_has_fk
          # should delete created record as otherwise disable_referential_integrity will try to enable constraints
          # after executed block and will fail (at least on Oracle)
          @connection.execute "DELETE FROM fk_test_has_fk"
        end
      end
    end

    private
      def insert_into_fk_test_has_fk(fk_id: 0)
        # Oracle adapter uses prefetched primary key values from sequence and passes them to connection adapter insert method
        if @connection.prefetch_primary_key?
          id_value = @connection.next_sequence_value(@connection.default_sequence_name("fk_test_has_fk", "id"))
          @connection.execute "INSERT INTO fk_test_has_fk (id,fk_id) VALUES (#{id_value},#{fk_id})"
        else
          @connection.execute "INSERT INTO fk_test_has_fk (fk_id) VALUES (#{fk_id})"
        end
      end
  end

  class AdapterTestWithoutTransaction < ActiveRecord::TestCase
    self.use_transactional_tests = false

    fixtures :posts, :authors, :author_addresses

    def setup
      @connection = ActiveRecord::Base.connection
    end

    def test_create_with_query_cache
      @connection.enable_query_cache!

      count = Post.count

      @connection.create("INSERT INTO posts(title, body) VALUES ('', '')")

      assert_equal count + 1, Post.count
    ensure
      reset_fixtures("posts")
      @connection.disable_query_cache!
    end

    def test_truncate
      assert_operator Post.count, :>, 0

      @connection.truncate("posts")

      assert_equal 0, Post.count
    ensure
      reset_fixtures("posts")
    end

    def test_truncate_with_query_cache
      @connection.enable_query_cache!

      assert_operator Post.count, :>, 0

      @connection.truncate("posts")

      assert_equal 0, Post.count
    ensure
      reset_fixtures("posts")
      @connection.disable_query_cache!
    end

    def test_truncate_tables
      assert_operator Post.count, :>, 0
      assert_operator Author.count, :>, 0
      assert_operator AuthorAddress.count, :>, 0

      @connection.truncate_tables("author_addresses", "authors", "posts")

      assert_equal 0, Post.count
      assert_equal 0, Author.count
      assert_equal 0, AuthorAddress.count
    ensure
      reset_fixtures("posts", "authors", "author_addresses")
    end

    def test_truncate_tables_with_query_cache
      @connection.enable_query_cache!

      assert_operator Post.count, :>, 0
      assert_operator Author.count, :>, 0
      assert_operator AuthorAddress.count, :>, 0

      @connection.truncate_tables("author_addresses", "authors", "posts")

      assert_equal 0, Post.count
      assert_equal 0, Author.count
      assert_equal 0, AuthorAddress.count
    ensure
      reset_fixtures("posts", "authors", "author_addresses")
      @connection.disable_query_cache!
    end

    def test_all_foreign_keys_valid_is_deprecated
      assert_deprecated(ActiveRecord.deprecator) do
        @connection.all_foreign_keys_valid?
      end
    end

    # test resetting sequences in odd tables in PostgreSQL
    if ActiveRecord::Base.connection.respond_to?(:reset_pk_sequence!)
      require "models/movie"
      require "models/subscriber"

      def test_reset_empty_table_with_custom_pk
        Movie.delete_all
        Movie.connection.reset_pk_sequence! "movies"
        assert_equal 1, Movie.create(name: "fight club").id
      end

      def test_reset_table_with_non_integer_pk
        Subscriber.delete_all
        Subscriber.connection.reset_pk_sequence! "subscribers"
        sub = Subscriber.new(name: "robert drake")
        sub.id = "bob drake"
        assert_nothing_raised { sub.save! }
      end
    end

    private
      def reset_fixtures(*fixture_names)
        ActiveRecord::FixtureSet.reset_cache

        fixture_names.each do |fixture_name|
          ActiveRecord::FixtureSet.create_fixtures(FIXTURES_ROOT, fixture_name)
        end
      end
  end

  class AdapterConnectionTest < ActiveRecord::TestCase
    unless in_memory_db?
      self.use_transactional_tests = false

      fixtures :posts, :authors, :author_addresses

      def setup
        @connection = ActiveRecord::Base.connection
        assert_predicate @connection, :active?
      end

      def teardown
        @connection.reconnect!
        assert_predicate @connection, :active?
        assert_not_predicate @connection, :transaction_open?
        assert_not raw_transaction_open?(@connection)
      end

      test "reconnect after a disconnect" do
        @connection.disconnect!
        assert_not_predicate @connection, :active?
        @connection.reconnect!
        assert_predicate @connection, :active?
      end

      test "materialized transaction state is reset after a reconnect" do
        @connection.begin_transaction
        assert_predicate @connection, :transaction_open?
        @connection.materialize_transactions
        assert raw_transaction_open?(@connection)
        @connection.reconnect!
        assert_not_predicate @connection, :transaction_open?
        assert_not raw_transaction_open?(@connection)
      end

      test "materialized transaction state can be restored after a reconnect" do
        @connection.begin_transaction
        assert_predicate @connection, :transaction_open?
        # +materialize_transactions+ currently automatically dirties the
        # connection, which would make it unrestorable
        @connection.transaction_manager.stub(:dirty_current_transaction, nil) do
          @connection.materialize_transactions
        end
        assert raw_transaction_open?(@connection)
        @connection.reconnect!(restore_transactions: true)
        assert_predicate @connection, :transaction_open?
        assert raw_transaction_open?(@connection)
      end

      test "materialized transaction state is reset after a disconnect" do
        @connection.begin_transaction
        assert_predicate @connection, :transaction_open?
        @connection.materialize_transactions
        assert raw_transaction_open?(@connection)
        @connection.disconnect!
        assert_not_predicate @connection, :transaction_open?
      end

      test "unmaterialized transaction state is reset after a reconnect" do
        @connection.begin_transaction
        assert_predicate @connection, :transaction_open?
        assert_not raw_transaction_open?(@connection)
        @connection.reconnect!
        assert_not_predicate @connection, :transaction_open?
        assert_not raw_transaction_open?(@connection)
        @connection.materialize_transactions
        assert_not raw_transaction_open?(@connection)
      end

      test "unmaterialized transaction state can be restored after a reconnect" do
        @connection.begin_transaction
        assert_predicate @connection, :transaction_open?
        assert_not raw_transaction_open?(@connection)
        @connection.reconnect!(restore_transactions: true)
        assert_predicate @connection, :transaction_open?
        assert_not raw_transaction_open?(@connection)
        @connection.materialize_transactions
        assert raw_transaction_open?(@connection)
      end

      test "unmaterialized transaction state is reset after a disconnect" do
        @connection.begin_transaction
        assert_predicate @connection, :transaction_open?
        assert_not raw_transaction_open?(@connection)
        @connection.disconnect!
        assert_not_predicate @connection, :transaction_open?
      end

      test "active? detects remote disconnection" do
        remote_disconnect @connection
        assert_not_predicate @connection, :active?
      end

      test "verify! restores after remote disconnection" do
        remote_disconnect @connection
        @connection.verify!
        assert_predicate @connection, :active?
      end

      test "reconnect! restores after remote disconnection" do
        remote_disconnect @connection
        @connection.reconnect!
        assert_predicate @connection, :active?
      end

      test "querying a 'clean' failed connection restores and succeeds" do
        remote_disconnect @connection

        @connection.clean! # this simulates a fresh checkout from the pool

        # Clean did not verify / fix the connection
        assert_not_predicate @connection, :active?

        # Because the connection hasn't been verified since checkout,
        # and the query cannot safely be retried, the connection will be
        # verified before querying.
        Post.delete_all

        assert_predicate @connection, :active?
      end

      test "transaction restores after remote disconnection" do
        remote_disconnect @connection
        Post.transaction do
          Post.count
        end
        assert_predicate @connection, :active?
      end

      test "active transaction is restored after remote disconnection" do
        assert_operator Post.count, :>, 0
        Post.transaction do
          # +materialize_transactions+ currently automatically dirties the
          # connection, which would make it unrestorable
          @connection.transaction_manager.stub(:dirty_current_transaction, nil) do
            @connection.materialize_transactions
          end

          remote_disconnect @connection

          # Regular queries are not retryable, so the only abstract operation we can
          # perform here is a direct verify. The outer transaction means using another
          # here would just be a ResetParent.
          @connection.verify!

          Post.delete_all

          assert_equal 0, Post.count
          raise ActiveRecord::Rollback
        end

        # The deletion occurred within the outer transaction (which was then rolled
        # back), and not directly on the freshly-reestablished connection, so the
        # posts are still there:
        assert_operator Post.count, :>, 0
      end

      test "dirty transaction cannot be restored after remote disconnection" do
        invocations = 0
        assert_raises ActiveRecord::ConnectionFailed do
          Post.transaction do
            invocations += 1
            Post.delete_all
            remote_disconnect @connection
            Post.count
          end
        end

        assert_equal 1, invocations # the whole transaction block is not retried

        # After the (outermost) transaction block failed, the connection is
        # ready to reconnect on next use, but hasn't done so yet
        assert_not_predicate @connection, :active?
        assert_operator Post.count, :>, 0
      end

      test "can reconnect and retry queries under limit when retry deadline is set" do
        attempts = 0
        @connection.stub(:retry_deadline, 0.1) do
          @connection.send(:with_raw_connection, allow_retry: true) do
            if attempts == 0
              attempts += 1
              raise ActiveRecord::ConnectionFailed.new("Something happened to the connection")
            end
          end
        end
      end

      test "does not reconnect and retry queries when retries are disabled" do
        assert_raises(ActiveRecord::ConnectionFailed) do
          attempts = 0
          @connection.send(:with_raw_connection) do
            if attempts == 0
              attempts += 1
              raise ActiveRecord::ConnectionFailed.new("Something happened to the connection")
            end
          end
        end
      end

      test "does not reconnect and retry queries that exceed retry deadline" do
        assert_raises(ActiveRecord::ConnectionFailed) do
          attempts = 0
          @connection.stub(:retry_deadline, 0.1) do
            @connection.send(:with_raw_connection, allow_retry: true) do
              if attempts == 0
                sleep(0.2)
                attempts += 1
                raise ActiveRecord::ConnectionFailed.new("Something happened to the connection")
              end
            end
          end
        end
      end

      test "#execute is retryable" do
        conn_id = case @connection.class::ADAPTER_NAME
                  when "Mysql2"
                    @connection.execute("SELECT CONNECTION_ID()").to_a[0][0]
                  when "Trilogy"
                    @connection.execute("SELECT CONNECTION_ID() as connection_id").to_a[0][0]
                  when "PostgreSQL"
                    @connection.execute("SELECT pg_backend_pid()").to_a[0]["pg_backend_pid"]
                  else
                    skip("kill_connection_from_server unsupported")
        end

        kill_connection_from_server(conn_id)

        @connection.execute("SELECT 1", allow_retry: true)
      end

      private
        def raw_transaction_open?(connection)
          case connection.class::ADAPTER_NAME
          when "PostgreSQL"
            connection.instance_variable_get(:@raw_connection).transaction_status == ::PG::PQTRANS_INTRANS
          when "Mysql2", "Trilogy"
            begin
              connection.instance_variable_get(:@raw_connection).query("SAVEPOINT transaction_test")
              connection.instance_variable_get(:@raw_connection).query("RELEASE SAVEPOINT transaction_test")

              true
            rescue
              false
            end
          when "SQLite"
            begin
              connection.instance_variable_get(:@raw_connection).transaction { nil }
              false
            rescue
              true
            end
          else
            skip("kill_connection_from_server unsupported")
          end
        end

        def remote_disconnect(connection)
          case connection.class::ADAPTER_NAME
          when "PostgreSQL"
            unless connection.instance_variable_get(:@raw_connection).transaction_status == ::PG::PQTRANS_INTRANS
              connection.instance_variable_get(:@raw_connection).async_exec("begin")
            end
            connection.instance_variable_get(:@raw_connection).async_exec("set idle_in_transaction_session_timeout = '10ms'")
            sleep 0.05
          when "Mysql2", "Trilogy"
            connection.send(:internal_execute, "set @@wait_timeout=1", materialize_transactions: false)
            sleep 1.2
          else
            skip("remote_disconnect unsupported")
          end
        end

        def kill_connection_from_server(connection_id)
          conn = @connection.pool.checkout
          case conn.class::ADAPTER_NAME
          when "Mysql2", "Trilogy"
            conn.execute("KILL #{connection_id}")
          when "PostgreSQL"
            conn.execute("SELECT pg_cancel_backend(#{connection_id})")
          else
            skip("kill_connection_from_server unsupported")
          end

          conn.close
        end
    end
  end
end

if ActiveRecord::Base.connection.supports_advisory_locks?
  class AdvisoryLocksEnabledTest < ActiveRecord::TestCase
    include ConnectionHelper

    def test_advisory_locks_enabled?
      assert ActiveRecord::Base.connection.advisory_locks_enabled?

      run_without_connection do |orig_connection|
        ActiveRecord::Base.establish_connection(
          orig_connection.merge(advisory_locks: false)
        )

        assert_not ActiveRecord::Base.connection.advisory_locks_enabled?

        ActiveRecord::Base.establish_connection(
          orig_connection.merge(advisory_locks: true)
        )

        assert ActiveRecord::Base.connection.advisory_locks_enabled?
      end
    end
  end
end

if ActiveRecord::Base.connection.savepoint_errors_invalidate_transactions?
  class InvalidateTransactionTest < ActiveRecord::TestCase
    def test_invalidates_transaction_on_rollback_error
      @invalidated = false
      connection = ActiveRecord::Base.connection

      connection.transaction do
        connection.send(:with_raw_connection) do
          raise ActiveRecord::Deadlocked, "made-up deadlock"
        end

      rescue ActiveRecord::Deadlocked => error
        flunk("Rescuing wrong error") unless error.message == "made-up deadlock"

        @invalidated = connection.current_transaction.invalidated?
      end

      # asserting outside of the transaction to make sure we actually reach the end of the test
      # and perform the assertion
      assert @invalidated
    end
  end
end
