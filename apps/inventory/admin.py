from django.contrib import admin
from .models import InventoryItem, InventoryTransaction

@admin.register(InventoryItem)
class InventoryItemAdmin(admin.ModelAdmin):
    list_display = ('name', 'sku', 'quantity', 'unit', 'price')
    search_fields = ('name', 'sku')

@admin.register(InventoryTransaction)
class InventoryTransactionAdmin(admin.ModelAdmin):
    list_display = ('item', 'transaction_type', 'quantity', 'timestamp')
    list_filter = ('transaction_type', 'timestamp')
