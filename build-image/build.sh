docker run -d \
    --name ink-container \
    --mount type=bind,source="$(pwd)",target="/contract" \
    parity/ver-build
