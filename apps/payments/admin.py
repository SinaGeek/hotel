from django.contrib import admin
from .models import Payment, CashShift

@admin.register(Payment)
class PaymentAdmin(admin.ModelAdmin):
    list_display = ('id', 'method', 'amount', 'status', 'folio')
    list_filter = ('method', 'status')

admin.site.register(CashShift)
