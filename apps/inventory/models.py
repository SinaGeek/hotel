from django.db import models
from apps.core.models import HotelScopedModel
from django.utils import timezone

class InventoryCategory(HotelScopedModel):
    name = models.CharField(max_length=100, unique=True)
    description = models.TextField(blank=True)
    # You can add permissions fields here if needed
    def __str__(self):
        return self.name

class InventoryItem(HotelScopedModel):
    # category foreign key defined later with null=True, blank=True
    name = models.CharField(max_length=120)
    sku = models.CharField(max_length=50, unique=True)
    image = models.ImageField(upload_to='inventory/items/', null=True, blank=True)
    quantity = models.DecimalField(max_digits=12, decimal_places=2, default=0)
    min_level = models.DecimalField(max_digits=12, decimal_places=2, default=5)
    unit = models.CharField(max_length=20, default='pcs')
    last_buy_price = models.DecimalField(max_digits=12, decimal_places=2, default=0)
    selling_price = models.DecimalField(max_digits=12, decimal_places=2, default=0)
    expiration_date = models.DateField(null=True, blank=True)
    is_consumable = models.BooleanField(default=False, verbose_name="Is Consumable (for rooms)")
    
    # For dynamic columns
    custom_data = models.JSONField(default=dict, blank=True)
    
    
    def save(self, *args, **kwargs):
        if not self.sku:
            # Automatic 6-digit SKU starting from 100000
            last_item = InventoryItem.objects.all().order_by('-sku').first()
            if last_item and last_item.sku.isdigit():
                self.sku = str(int(last_item.sku) + 1)
            else:
                self.sku = "100000"
        super().save(*args, **kwargs)

    def delete(self, *args, **kwargs):
        # Enforce history preservation: Deletion is strictly prohibited
        raise PermissionError("Inventory items cannot be deleted to preserve historical records.")

    @property
    def days_to_expiry(self):
        if self.expiration_date:
            delta = self.expiration_date - timezone.now().date()
            return delta.days
        return None

    @property
    def is_expired(self):
        days = self.days_to_expiry
        return days is not None and days <= 0

    @property
    def is_below_min(self):
        return self.quantity < self.min_level

class InventoryTransaction(HotelScopedModel):
    TRANSACTION_TYPES = (
        ('ISSUE', 'Issue Material'),
        ('RETURN', 'Return Material'),
        ('REQUEST', 'Request Material'),
        ('BUY', 'Stock Buy'),
        ('ADJ', 'Adjustment'),
    )
    item = models.ForeignKey(InventoryItem, on_delete=models.CASCADE, related_name='transactions')
    transaction_type = models.CharField(max_length=10, choices=TRANSACTION_TYPES)
    quantity = models.DecimalField(max_digits=12, decimal_places=2)
    user = models.ForeignKey('accounts.User', on_delete=models.SET_NULL, null=True)
    timestamp = models.DateTimeField(auto_now_add=True)
    notes = models.TextField(blank=True)

class InventoryColumn(HotelScopedModel):
    name = models.CharField(max_length=50)
    key = models.SlugField(max_length=50, unique=True)
    formula = models.CharField(max_length=200, blank=True, help_text="Simple JS-like formula referencing other keys")
    is_active = models.BooleanField(default=True)
