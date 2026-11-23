# Build locally in the extracted release archive: docker build -t hune:<version> .
# Never push this image to a registry.
FROM scratch
COPY --chmod=0555 hune /hune
USER 65532:65532
ENTRYPOINT ["/hune"]
CMD ["serve", "-c", "/etc/hune/config.hcl"]
HEALTHCHECK --interval=10s --timeout=3s --start-period=20s --retries=3 \
  CMD ["/hune", "health", "-c", "/etc/hune/config.hcl"]
