Synthetic TLS fixtures for the tests of hune-source. They protect nothing.
ca.pem          test CA (valid until 2126)
server.pem      server certificate for DNS clickhouse-test and IP 127.0.0.1, signed by ca.pem
server.key      the key of server.pem (PKCS#8, test only)
other-ca.pem    a CA that did not sign server.pem
The CA keys were deleted after signing.
