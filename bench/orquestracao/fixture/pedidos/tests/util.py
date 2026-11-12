import os
import tempfile

CABECALHO = "id,cliente,data,sku,qtd,preco_unit,categoria\n"


def csv_temp(conteudo: str, cabecalho: str = CABECALHO) -> str:
    """Grava um CSV temporário e devolve o caminho."""
    fd, caminho = tempfile.mkstemp(suffix=".csv")
    with os.fdopen(fd, "w", encoding="utf-8") as f:
        f.write(cabecalho + conteudo)
    return caminho
