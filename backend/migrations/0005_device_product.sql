-- 0005 — the app's device row must not join a vendor-scoped table.
--
-- `products` is protected by vendor RLS: a customer request has no vendor, so a
-- join to it returns nothing and the customer's own devices disappear. The
-- product's identity is therefore denormalised onto the device at claim time.
-- It cannot drift: a published product is immutable, and a live device is never
-- re-pointed at a different product (a new product_key is a new row).

ALTER TABLE devices ADD COLUMN product_key text;
ALTER TABLE devices ADD COLUMN category text;

UPDATE devices d
   SET product_key = p.product_key,
       category    = p.category
  FROM products p
 WHERE p.id = d.product_id;

ALTER TABLE devices ALTER COLUMN product_key SET NOT NULL;
ALTER TABLE devices ALTER COLUMN category SET NOT NULL;

CREATE INDEX devices_product_key_idx ON devices (product_key);
