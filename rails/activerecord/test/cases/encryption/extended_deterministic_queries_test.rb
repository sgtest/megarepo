# frozen_string_literal: true

require "cases/encryption/helper"
require "models/book_encrypted"

class ActiveRecord::Encryption::ExtendedDeterministicQueriesTest < ActiveRecord::EncryptionTestCase
  setup do
    ActiveRecord::Encryption.config.support_unencrypted_data = true
  end

  test "Finds records when data is unencrypted" do
    ActiveRecord::Encryption.without_encryption { UnencryptedBook.create! name: "Dune" }
    assert EncryptedBook.find_by(name: "Dune") # core
    assert EncryptedBook.where("id > 0").find_by(name: "Dune") # relation
  end

  test "Finds records when data is encrypted" do
    UnencryptedBook.create! name: "Dune"
    assert EncryptedBook.find_by(name: "Dune") # core
    assert EncryptedBook.where("id > 0").find_by(name: "Dune") # relation
  end

  test "Works well with downcased attributes" do
    ActiveRecord::Encryption.without_encryption { EncryptedBookWithDowncaseName.create! name: "Dune" }
    assert EncryptedBookWithDowncaseName.find_by(name: "DUNE")
  end

  test "Works well with string attribute names" do
    UnencryptedBook.create! "name" => "Dune"
    assert EncryptedBook.find_by("name" => "Dune")
  end

  test "find_or_create_by works" do
    EncryptedBook.find_or_create_by!(name: "Dune")
    assert EncryptedBook.find_by(name: "Dune")

    EncryptedBook.find_or_create_by!(name: "Dune")
    assert EncryptedBook.find_by(name: "Dune")
  end

  test "does not mutate arguments" do
    props = { name: "Dune" }

    assert_equal "Dune", EncryptedBook.find_or_initialize_by(props).name
    assert_equal "Dune", props[:name]
  end

  test "where(...).first_or_create works" do
    EncryptedBook.where(name: "Dune").first_or_create
    assert EncryptedBook.exists?(name: "Dune")
  end

  test "exists?(...) works" do
    ActiveRecord::Encryption.without_encryption { EncryptedBook.create! name: "Dune" }
    assert EncryptedBook.exists?(name: "Dune")
  end
end
