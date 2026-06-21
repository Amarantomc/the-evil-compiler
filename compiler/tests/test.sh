# set -e

# # Run the Rust project to generate output.ll
# cargo run

# # Compile the LLVM IR into an executable
# #clang output.ll -isysroot $(xcrun --show-sdk-path) -o hulk_executable
# llc -filetype=obj output.ll -o output.obj 
# gcc output.obj -o output.exe  

# # Run the resulting executable
# ./output.exe 

#!/bin/bash

# Configura aquí la ruta de la carpeta que contiene tus archivos .hulk y .expected
TEST_DIR="../tests"

# Contadores para el reporte final
PASSED=0
FAILED=0
FAILED_TESTS=()

echo "🚀 Iniciando suite de pruebas para HULK..."
echo "----------------------------------------"

# Iterar sobre cada archivo .hulk en la carpeta de pruebas
for test_file in "$TEST_DIR"/*.hulk; do
    # Validar si existen archivos (evita errores si la carpeta está vacía)
    [ -e "$test_file" ] || { echo "No se encontraron archivos .hulk en '$TEST_DIR'"; exit 1; }

    # Obtener el nombre base sin extensión para buscar su respectivo .expected
    base_name="${test_file%.hulk}"
    expected_file="${base_name}.exit"
    test_name=$(basename "$test_file")

    echo -n "Test [$test_name]... "

    # 1. Verificar si existe el archivo .expected correspondiente
    if [ ! -f "$expected_file" ]; then
        echo "❌ FALLÓ (Falta el archivo .expected)"
        ((FAILED++))
        continue
    fi

    # 2. Ejecutar el compilador pasando el archivo actual como argumento
    # Usamos '--' para pasarle el argumento directamente a tu binario de Rust
    cargo run $test_name
    EXIT_CODE=$?
    echo "Código de salida del compilador: $EXIT_CODE"
    

    # 5. Ejecutar el binario y capturar su salida en un archivo temporal
    ./output.exe > output.actual 2>&1

    # 6. Comparar el output real con el esperado usando 'diff'
     if [ $EXIT_CODE -eq 3 ]; then
        echo "✅ PASÓ (Falló como se esperaba con código 3)"
        ((PASSED++))
    else
        echo "❌ FALLÓ (Se esperaba código 1 pero se obtuvo $EXIT_CODE)"
        ((FAILED++))
        FAILED_TESTS+=("$test_name")
    fi
done

echo "----------------------------------------"
echo "📊 Resumen: $PASSED pasados, $FAILED fallidos."
if [ ${#FAILED_TESTS[@]} -ne 0 ]; then
    echo -e "\n❌ Lista de casos fallidos:"
    for failed in "${FAILED_TESTS[@]}"; do
        echo "  - $failed"
    done
fi

# Limpieza de los archivos temporales generados durante el proceso
rm -f output.ll output.obj output.exe output.actual output.o
 