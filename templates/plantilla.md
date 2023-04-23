Hello {{nombre}}
Tu factura es de {{m3}} m3, en el periodo {{periodo}}.
{% for concepto in conceptos %}{{concepto.nombre}} -- {{concepto.importe}}{% endfor %}
{{importes|first|round(method="ceil", precision=2)}}
Y te va a salir por {{eur|round(method="ceil", precision=2)}} Euros. Waw.
---
