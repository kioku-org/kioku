-- API keys now generate as kioku_<hex> instead of cmp_<hex>. Widen key_prefix
-- to fit the longer prefix; existing cmp_ rows and their shorter prefixes keep
-- working unchanged (validation accepts both formats going forward).
ALTER TABLE company_api_keys ALTER COLUMN key_prefix TYPE VARCHAR(20);
