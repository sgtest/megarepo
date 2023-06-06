# frozen_string_literal: true

ActiveRecord::Schema.define do
  ActiveRecord::TestCase.enable_extension!("uuid-ossp", ActiveRecord::Base.connection)
  ActiveRecord::TestCase.enable_extension!("pgcrypto",  ActiveRecord::Base.connection) if ActiveRecord::Base.connection.supports_pgcrypto_uuid?

  uuid_default = connection.supports_pgcrypto_uuid? ? {} : { default: "uuid_generate_v4()" }

  create_table :chat_messages, id: :uuid, force: true, **uuid_default do |t|
    t.text :content
  end

  create_table :chat_messages_custom_pk, id: false, force: true do |t|
    t.uuid :message_id, primary_key: true, default: "uuid_generate_v4()"
    t.text :content
  end

  create_table :uuid_parents, id: :uuid, force: true, **uuid_default do |t|
    t.string :name
  end

  create_table :uuid_children, id: :uuid, force: true, **uuid_default do |t|
    t.string :name
    t.uuid :uuid_parent_id
  end

  create_table :defaults, force: true do |t|
    t.virtual :virtual_stored_number, type: :integer, as: "rand_number * 10", stored: true
    t.integer :rand_number, default: -> { "random() * 100" }
    t.string :ruby_on_rails, default: -> { "concat('Ruby ', 'on ', 'Rails')" }
    t.date :modified_date, default: -> { "CURRENT_DATE" }
    t.date :modified_date_function, default: -> { "now()" }
    t.date :fixed_date, default: "2004-01-01"
    t.datetime :modified_time, default: -> { "CURRENT_TIMESTAMP" }
    t.datetime :modified_time_without_precision, precision: nil, default: -> { "CURRENT_TIMESTAMP" }
    t.datetime :modified_time_with_precision_0, precision: 0, default: -> { "CURRENT_TIMESTAMP" }
    t.datetime :modified_time_function, default: -> { "now()" }
    t.datetime :fixed_time, default: "2004-01-01 00:00:00.000000-00"
    t.timestamptz :fixed_time_with_time_zone, default: "2004-01-01 01:00:00+1"
    t.column :char1, "char(1)", default: "Y"
    t.string :char2, limit: 50, default: "a varchar field"
    t.text :char3, default: "a text field"
    t.bigint :bigint_default, default: -> { "0::bigint" }
    t.text :multiline_default, default: "--- []

"
  end

  create_table :postgresql_times, force: true do |t|
    t.interval :time_interval
    t.interval :scaled_time_interval, precision: 6
  end

  create_table :postgresql_oids, force: true do |t|
    t.oid :obj_id
  end

  drop_table "postgresql_timestamp_with_zones", if_exists: true
  drop_table "postgresql_partitioned_table", if_exists: true
  drop_table "postgresql_partitioned_table_parent", if_exists: true

  execute "DROP SEQUENCE IF EXISTS companies_nonstd_seq CASCADE"
  execute "CREATE SEQUENCE companies_nonstd_seq START 101 OWNED BY companies.id"
  execute "ALTER TABLE companies ALTER COLUMN id SET DEFAULT nextval('companies_nonstd_seq')"
  execute "DROP SEQUENCE IF EXISTS companies_id_seq"

  execute "DROP FUNCTION IF EXISTS partitioned_insert_trigger()"

  %w(accounts_id_seq developers_id_seq projects_id_seq topics_id_seq customers_id_seq orders_id_seq).each do |seq_name|
    execute "SELECT setval('#{seq_name}', 100)"
  end

  execute <<_SQL
  CREATE TABLE postgresql_timestamp_with_zones (
    id SERIAL PRIMARY KEY,
    time TIMESTAMP WITH TIME ZONE
  );
_SQL

  begin
    execute <<_SQL
    CREATE TABLE postgresql_partitioned_table_parent (
      id SERIAL PRIMARY KEY,
      number integer
    );
    CREATE TABLE postgresql_partitioned_table ( )
      INHERITS (postgresql_partitioned_table_parent);

    CREATE OR REPLACE FUNCTION partitioned_insert_trigger()
    RETURNS TRIGGER AS $$
    BEGIN
      INSERT INTO postgresql_partitioned_table VALUES (NEW.*);
      RETURN NULL;
    END;
    $$
    LANGUAGE plpgsql;

    CREATE TRIGGER insert_partitioning_trigger
      BEFORE INSERT ON postgresql_partitioned_table_parent
      FOR EACH ROW EXECUTE PROCEDURE partitioned_insert_trigger();
_SQL
  rescue ActiveRecord::StatementInvalid => e
    if e.message.include?('language "plpgsql" does not exist')
      execute "CREATE LANGUAGE 'plpgsql';"
      retry
    else
      raise e
    end
  end

  # This table is to verify if the :limit option is being ignored for text and binary columns
  create_table :limitless_fields, force: true do |t|
    t.binary :binary, limit: 100_000
    t.text :text, limit: 100_000
  end

  create_table :bigint_array, force: true do |t|
    t.integer :big_int_data_points, limit: 8, array: true
    t.decimal :decimal_array_default, array: true, default: [1.23, 3.45]
  end

  create_table :uuid_comments, force: true, id: false do |t|
    t.uuid :uuid, primary_key: true, **uuid_default
    t.string :content
  end

  create_table :uuid_entries, force: true, id: false do |t|
    t.uuid :uuid, primary_key: true, **uuid_default
    t.string :entryable_type, null: false
    t.uuid :entryable_uuid, null: false
  end

  create_table :uuid_items, force: true, id: false do |t|
    t.uuid :uuid, primary_key: true, **uuid_default
    t.string :title
  end

  create_table :uuid_messages, force: true, id: false do |t|
    t.uuid :uuid, primary_key: true, **uuid_default
    t.string :subject
  end

  create_table :test_exclusion_constraints, force: true do |t|
    t.date :start_date
    t.date :end_date
    t.date :valid_from
    t.date :valid_to
    t.date :transaction_from
    t.date :transaction_to

    t.exclusion_constraint "daterange(start_date, end_date) WITH &&", using: :gist, where: "start_date IS NOT NULL AND end_date IS NOT NULL", name: "test_exclusion_constraints_date_overlap"
    t.exclusion_constraint "daterange(valid_from, valid_to) WITH &&", using: :gist, where: "valid_from IS NOT NULL AND valid_to IS NOT NULL", name: "test_exclusion_constraints_valid_overlap", deferrable: :immediate
    t.exclusion_constraint "daterange(transaction_from, transaction_to) WITH &&", using: :gist, where: "transaction_from IS NOT NULL AND transaction_to IS NOT NULL", name: "test_exclusion_constraints_transaction_overlap", deferrable: :deferred
  end

  create_table :test_unique_keys, force: true do |t|
    t.integer :position_1
    t.integer :position_2
    t.integer :position_3

    t.unique_key :position_1, name: "test_unique_keys_position_deferrable_false"
    t.unique_key :position_2, name: "test_unique_keys_position_deferrable_immediate", deferrable: :immediate
    t.unique_key :position_3, name: "test_unique_keys_position_deferrable_deferred", deferrable: :deferred
  end

  if supports_partitioned_indexes?
    create_table(:measurements, id: false, force: true, options: "PARTITION BY LIST (city_id)") do |t|
      t.string :city_id, null: false
      t.date :logdate, null: false
      t.integer :peaktemp
      t.integer :unitsales
      t.index [:logdate, :city_id], unique: true
    end
    create_table(:measurements_toronto, id: false, force: true,
                                        options: "PARTITION OF measurements FOR VALUES IN (1)")
    create_table(:measurements_concepcion, id: false, force: true,
                                           options: "PARTITION OF measurements FOR VALUES IN (2)")
  end

  if supports_index_include?
    add_index(:companies, [:firm_id, :type], name: "company_include_index", include: [:name, :account_id])
  end
end
