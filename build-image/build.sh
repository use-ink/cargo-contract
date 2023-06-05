docker run -d \
    --name flipper \
    --mount type=bind,source="$(pwd)",target="/contract" \
    parity/ver-build
