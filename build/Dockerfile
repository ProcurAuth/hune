# Build locally in the extracted release archive: docker build -t hune:<version> .
# Never push this image to a registry.
FROM scratch
COPY --chmod=0555 hune /hune
LABEL org.opencontainers.image.licenses="MIT OR Apache-2.0"
USER 65532:65532
STOPSIGNAL SIGTERM
ENTRYPOINT ["/hune"]
CMD ["serve", "-c", "/etc/hune/config.hcl"]
HEALTHCHECK --interval=10s --timeout=3s --start-period=20s --retries=3 \
  CMD ["/hune", "health", "-c", "/etc/hune/config.hcl"]
