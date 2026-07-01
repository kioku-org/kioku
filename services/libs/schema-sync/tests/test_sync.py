from unittest.mock import MagicMock, patch

from sqlalchemy import Column, Integer, MetaData, String
from sqlalchemy.orm import declarative_base

from schema_sync import sync


Base = declarative_base(metadata=MetaData(schema="vexa"))


class Widget(Base):
    __tablename__ = "widgets"

    id = Column(Integer, primary_key=True)
    name = Column(String(32), nullable=False)


class Gadget(Base):
    __tablename__ = "gadgets"

    id = Column(Integer, primary_key=True)
    tier = Column(String(16), nullable=False, server_default="free")


def test_ensure_schemas_creates_declared_schema():
    conn = MagicMock()

    sync._ensure_schemas(conn, Base)

    executed = conn.execute.call_args.args[0]
    assert executed.text == 'CREATE SCHEMA IF NOT EXISTS "vexa"'


def test_sync_columns_uses_schema_aware_introspection_and_qualified_alter():
    conn = MagicMock()
    inspector = MagicMock()
    inspector.get_table_names.return_value = ["widgets"]
    inspector.get_columns.return_value = [{"name": "id"}]

    with patch("schema_sync.sync.inspect", return_value=inspector):
        sync._sync_columns(conn, Base)

    inspector.get_table_names.assert_called_once_with(schema="vexa")
    inspector.get_columns.assert_called_once_with("widgets", schema="vexa")
    executed = conn.execute.call_args.args[0]
    assert executed.text == (
        'ALTER TABLE "vexa"."widgets" ADD COLUMN "name" VARCHAR(32) NOT NULL DEFAULT \'\''
    )


def test_col_default_sql_quotes_plain_string_server_default():
    """Regression test: an unquoted `DEFAULT free` is invalid SQL — Postgres parses
    the bare word as a column/function reference and rejects the ALTER TABLE."""
    col = Gadget.__table__.c.tier

    assert sync._col_default_sql(col) == " DEFAULT 'free'"


def test_col_default_sql_escapes_embedded_quotes():
    class _FakeServerDefault:
        arg = "o'brien"

    col = MagicMock()
    col.server_default = _FakeServerDefault()

    assert sync._col_default_sql(col) == " DEFAULT 'o''brien'"


def test_sync_columns_quotes_string_server_default_in_alter_statement():
    conn = MagicMock()
    inspector = MagicMock()
    inspector.get_table_names.return_value = ["gadgets"]
    inspector.get_columns.return_value = [{"name": "id"}]

    with patch("schema_sync.sync.inspect", return_value=inspector):
        sync._sync_columns(conn, Base)

    executed = conn.execute.call_args.args[0]
    assert executed.text == (
        'ALTER TABLE "vexa"."gadgets" ADD COLUMN "tier" VARCHAR(16) NOT NULL DEFAULT \'free\''
    )
